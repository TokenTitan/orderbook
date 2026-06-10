use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::orders::{Order, OrderId, Price, Side};

#[derive(Default)]
pub struct OrderBook {
    bids: BTreeMap<Price, VecDeque<Order>>,
    asks: BTreeMap<Price, VecDeque<Order>>,
    index: HashMap<OrderId, (Side, Price)>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    pub fn spread(&self) -> Option<u64> {
        let best_bid = self.best_bid()?;
        let best_ask = self.best_ask()?;

        best_ask.checked_sub(best_bid)
    }

    pub fn add_order(&mut self, order: Order) {
        let order_id = order.id;
        let side = order.side;
        let price = order.price;

        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        book.entry(price).or_default().push_back(order);
        self.index.insert(order_id, (side, price));
    }

    pub fn cancel_order(&mut self, id: OrderId) -> bool {
        let (side, price) = match self.index.remove(&id) {
            Some((side, price)) => (side, price),
            None => return false,
        };

        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        let Some(orders) = book.get_mut(&price) else {
            return false;
        };

        let Some(position) = orders.iter().position(|order| order.id == id) else {
            return false;
        };

        orders.remove(position);

        if orders.is_empty() {
            book.remove(&price);
        }

        true
    }
}
