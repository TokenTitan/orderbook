use phase4_orderbook::{
    orderbook::OrderBook,
    orders::{Order, Side},
};

fn main() {
    let mut book = OrderBook::new();

    // Seed the book with resting limit orders
    book.add_order(Order::limit(1, Side::Bid, 69_200, 5));
    book.add_order(Order::limit(2, Side::Bid, 69_300, 3));
    book.add_order(Order::limit(3, Side::Bid, 69_300, 2)); // same level as order 2 — FIFO
    book.add_order(Order::limit(4, Side::Ask, 69_500, 4));
    book.add_order(Order::limit(5, Side::Ask, 69_600, 8));
    book.add_order(Order::limit(6, Side::Ask, 69_700, 2));

    println!("\n--- Initial book ---");
    book.display();

    // Cancel one resting bid
    println!("\n--- Cancelling order 2 (bid @ 69300 qty 3) ---");
    book.cancel_order(2);
    book.display();

    // Aggressive limit bid that crosses the spread — will match against asks
    println!("\n--- Submitting limit bid @ 69600 qty 6 ---");
    let fills = book.submit_order(Order::limit(7, Side::Bid, 69_600, 6));
    for f in &fills {
        println!(
            "  FILL  maker={} qty={} exec_price={}",
            f.maker_order_id, f.quantity, f.execution_price
        );
    }
    book.display();

    // Market order — no price, takes whatever is available
    println!("\n--- Submitting market sell qty 3 ---");
    let fills = book.submit_order(Order::market(8, Side::Ask, 3));
    for f in &fills {
        println!(
            "  FILL  maker={} qty={} exec_price={}",
            f.maker_order_id, f.quantity, f.execution_price
        );
    }
    book.display();
}
