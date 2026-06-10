use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::orders::{Order, OrderId, OrderType, Price, Quantity, Side};

// Why BTreeMap for price levels?
// - Keeps prices sorted automatically — O(log n) insert/remove
// - best_bid = last key, best_ask = first key — O(log n) access
// - Sorted iteration for matching (walk from best price inward)
// If we used HashMap: O(1) insert but O(n) to find best price. Wrong trade-off.
//
// Why VecDeque per price level?
// - FIFO within a level: push_back on arrival, pop_front on fill
// - O(1) access to the front (next order to fill)
// - O(n) for cancel by ID — see cancel_order for the trade-off discussion
#[derive(Default)]
pub struct OrderBook {
    bids: BTreeMap<Price, VecDeque<Order>>,
    asks: BTreeMap<Price, VecDeque<Order>>,
    // Secondary index: O(1) lookup of which side+price level an order lives in.
    // Essential for cancel_order — without this we'd scan every price level.
    index: HashMap<OrderId, (Side, Price)>,
}

// A Fill records one execution event.
// execution_price is the MAKER's price — standard price-time priority behaviour.
// Example: bid at $70,000 matches resting ask at $69,500 → executes at $69,500.
// The aggressive (taker) order gets price improvement. The passive (maker) order
// gets the price it asked for.
#[derive(Debug)]
pub struct Fill {
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub execution_price: Price, // maker's price — where the trade actually happens
    pub quantity: Quantity,
}

impl OrderBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn best_bid(&self) -> Option<Price> {
        // last() = highest key = highest bid price
        self.bids.keys().next_back().copied()
    }

    pub fn best_ask(&self) -> Option<Price> {
        // first() = lowest key = lowest ask price (cheapest seller)
        self.asks.keys().next().copied()
    }

    // checked_sub handles crossed books: if ask < bid (shouldn't happen in a
    // healthy book), returns None instead of wrapping around on u64.
    pub fn spread(&self) -> Option<u64> {
        let best_bid = self.best_bid()?;
        let best_ask = self.best_ask()?;
        best_ask.checked_sub(best_bid)
    }

    // Rests an order in the book without matching.
    // Used internally by submit_order for unmatched remainder.
    pub fn add_order(&mut self, order: Order) {
        let order_id = order.id;
        let side = order.side;
        let price = order.price;

        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        // or_default() creates an empty VecDeque if this price level doesn't exist yet.
        book.entry(price).or_default().push_back(order);
        self.index.insert(order_id, (side, price));
    }

    // Cancel an order by ID. Returns false if the order doesn't exist.
    //
    // Current complexity: O(log n) for BTreeMap lookup + O(k) scan within the
    // price level (k = orders at that price). For most books k is small.
    //
    // Production O(1) alternative — lazy deletion:
    //   Add a `cancelled: HashSet<OrderId>` field.
    //   cancel_order just inserts into the set — O(1).
    //   During matching, skip any order whose ID is in the set.
    //   Trade-off: cancelled orders occupy memory until they're skipped during matching.
    //   Used by exchanges with very high cancel rates (market makers cancel 90%+ of orders).
    pub fn cancel_order(&mut self, id: OrderId) -> bool {
        let (side, price) = match self.index.get(&id).copied() {
            Some(entry) => entry,
            None => return false,
        };

        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        // These branches are unreachable if the book is in a consistent state.
        // We panic rather than silently returning false — a silent false would hide
        // a bug (index out of sync with book). Panicking surfaces the inconsistency.
        let orders = book
            .get_mut(&price)
            .expect("index points to price level that doesn't exist — data inconsistency");

        let position = orders
            .iter()
            .position(|o| o.id == id)
            .expect("index points to order that isn't in its price level — data inconsistency");

        orders.remove(position);

        // Clean up the price level if it's now empty.
        // Important: leaving empty VecDeques in the BTreeMap would cause
        // best_bid/best_ask to return a price with no orders behind it.
        if orders.is_empty() {
            book.remove(&price);
        }

        self.index.remove(&id);
        true
    }

    // Submit an order for matching. Returns all fills produced.
    //
    // Matching rules:
    // - Aggressive (taker) order walks the opposite side from best price inward
    // - Each price level is consumed FIFO (time priority)
    // - Partial fills allowed: resting order loses quantity and stays in book
    // - After matching, any remaining quantity is rested (limit) or discarded (market)
    pub fn submit_order(&mut self, mut order: Order) -> Vec<Fill> {
        let mut fills = Vec::new();
        let taker_id = order.id;

        while order.quantity > 0 {
            // Find the best opposing price that crosses this order.
            // For a bid: lowest ask <= order price
            // For an ask: highest bid >= order price
            let opposite_price = match order.side {
                Side::Bid => self
                    .asks
                    .keys()
                    .next()
                    .copied()
                    .filter(|&p| p <= order.price),
                Side::Ask => self
                    .bids
                    .keys()
                    .next_back()
                    .copied()
                    .filter(|&p| p >= order.price),
            };

            let Some(opposite_price) = opposite_price else {
                break; // no more matching prices — done matching
            };

            let book = match order.side {
                Side::Bid => &mut self.asks,
                Side::Ask => &mut self.bids,
            };

            // Collect IDs of fully-filled resting orders so we can remove them
            // from the index after the borrow on `book` is released.
            let mut completed_ids: Vec<OrderId> = Vec::new();
            let level_empty;

            {
                let level = book
                    .get_mut(&opposite_price)
                    .expect("matching price level must exist");

                // Fill orders at this level FIFO until taker is filled or level exhausted.
                while order.quantity > 0 {
                    let Some(resting) = level.front_mut() else {
                        break;
                    };

                    let fill_qty = order.quantity.min(resting.quantity);

                    fills.push(Fill {
                        maker_order_id: resting.id,
                        taker_order_id: taker_id,
                        // Trade executes at the maker's price — standard price-time priority.
                        execution_price: resting.price,
                        quantity: fill_qty,
                    });

                    order.quantity -= fill_qty;
                    resting.quantity -= fill_qty;

                    if resting.quantity == 0 {
                        let done = level.pop_front().expect("just checked front exists");
                        completed_ids.push(done.id);
                    }
                }

                level_empty = level.is_empty();
            }

            // Borrow released — safe to mutate book and index now.
            if level_empty {
                book.remove(&opposite_price);
            }

            for id in completed_ids {
                self.index.remove(&id);
            }
        }

        // Rest any unmatched quantity, but not for market orders.
        // A market order may fill partially; any remainder is discarded.
        if order.quantity > 0 && order.order_type == OrderType::Limit {
            self.add_order(order);
        }

        fills
    }

    // Print the order book — useful for debugging and demos.
    // Shows asks top-down (highest first), then bids top-down (highest first).
    pub fn display(&self) {
        println!("┌─────────────────────────────────┐");
        println!("│           ORDER BOOK            │");
        println!("├────────┬────────────┬───────────┤");
        println!("│  SIDE  │   PRICE    │    QTY    │");
        println!("├────────┼────────────┼───────────┤");

        // Asks displayed from highest to lowest so the spread is in the middle
        for (price, orders) in self.asks.iter().rev() {
            let qty: Quantity = orders.iter().map(|o| o.quantity).sum();
            println!("│  ASK   │ {:>10} │ {:>9} │", price, qty);
        }

        println!("├────────┼────────────┼───────────┤");

        match self.spread() {
            Some(s) => println!("│        │  spread={:<4}│           │", s),
            None => println!("│        │  spread=n/a │           │"),
        }

        println!("├────────┼────────────┼───────────┤");

        // Bids from highest to lowest
        for (price, orders) in self.bids.iter().rev() {
            let qty: Quantity = orders.iter().map(|o| o.quantity).sum();
            println!("│  BID   │ {:>10} │ {:>9} │", price, qty);
        }

        println!("└────────┴────────────┴───────────┘");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(id: OrderId, side: Side, price: Price, qty: Quantity) -> Order {
        Order::limit(id, side, price, qty)
    }

    #[test]
    fn incoming_ask_matches_highest_bids_fifo() {
        let mut book = OrderBook::new();
        book.add_order(limit(1, Side::Bid, 100, 3));
        book.add_order(limit(2, Side::Bid, 100, 4));
        book.add_order(limit(3, Side::Bid, 90, 5));

        // Ask at 90 — matches bid at 100 first (better price), then bid at 90
        let fills = book.submit_order(limit(4, Side::Ask, 90, 8));

        assert_eq!(fills.len(), 3);
        // FIFO at price 100: order 1 first
        assert_eq!(fills[0].maker_order_id, 1);
        assert_eq!(fills[0].quantity, 3);
        assert_eq!(fills[0].execution_price, 100); // executes at maker's price
        assert_eq!(fills[1].maker_order_id, 2);
        assert_eq!(fills[1].quantity, 4);
        // Taker's 8 filled 3+4=7, moves to next level
        assert_eq!(fills[2].maker_order_id, 3);
        assert_eq!(fills[2].quantity, 1); // partial fill on order 3
        // Order 3 still resting with 4 remaining
        assert_eq!(book.best_bid(), Some(90));
        assert_eq!(book.bids[&90].front().unwrap().quantity, 4);
    }

    #[test]
    fn unmatched_limit_rests_on_book() {
        let mut book = OrderBook::new();
        let fills = book.submit_order(limit(1, Side::Bid, 100, 5));
        assert!(fills.is_empty());
        assert_eq!(book.best_bid(), Some(100));
        assert_eq!(book.index.get(&1), Some(&(Side::Bid, 100)));
    }

    #[test]
    fn market_order_does_not_rest() {
        let mut book = OrderBook::new();
        book.add_order(limit(1, Side::Ask, 100, 3)); // only 3 available

        // Market buy for 10 — only 3 available, remainder discarded
        let fills = book.submit_order(Order::market(2, Side::Bid, 10));

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].quantity, 3);
        // Market order must NOT rest — book should be empty
        assert!(
            book.best_bid().is_none(),
            "market order must not rest in book"
        );
        assert!(
            !book.index.contains_key(&2),
            "market order id must not be in index"
        );
    }

    #[test]
    fn cancel_removes_order_and_cleans_price_level() {
        let mut book = OrderBook::new();
        book.add_order(limit(1, Side::Bid, 100, 5));
        book.add_order(limit(2, Side::Bid, 100, 3));

        assert!(book.cancel_order(1));
        // Order 2 still at 100
        assert_eq!(book.best_bid(), Some(100));
        assert_eq!(book.bids[&100].len(), 1);

        assert!(book.cancel_order(2));
        // Price level 100 should be gone — no empty VecDeques in the map
        assert!(book.best_bid().is_none());
        assert!(!book.bids.contains_key(&100));

        // Cancelling non-existent order returns false
        assert!(!book.cancel_order(99));
    }

    #[test]
    fn execution_price_is_makers_price() {
        let mut book = OrderBook::new();
        // Resting ask at 69_500
        book.add_order(limit(1, Side::Ask, 69_500, 1));

        // Aggressive bid at 70_000 — willing to pay more, but trades at maker's price
        let fills = book.submit_order(limit(2, Side::Bid, 70_000, 1));

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].execution_price, 69_500); // not 70_000
    }
}
