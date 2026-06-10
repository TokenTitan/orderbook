use phase4_orderbook::{
    orderbook::OrderBook,
    orders::{Order, Side},
};

fn main() {
    let mut book = OrderBook::new();

    for i in 0..4 {
        book.add_order(Order {
            id: i as u64,
            side: Side::Bid,
            price: i * 100,
            quantity: i * 10,
        });

        book.add_order(Order {
            id: i + 4,
            side: Side::Ask,
            price: 400 + i * 110,
            quantity: i * 10,
        });
    }

    match (book.best_ask(), book.best_bid(), book.spread()) {
        (Some(ask), Some(bid), Some(spread)) => {
            println!("Ask: {ask}, Bid: {bid}, Spread: {spread}");
        }
        _ => {
            println!("The order book does not have enough orders");
        }
    }
}
