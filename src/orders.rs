pub type Price = u64;
pub type Quantity = u64;
pub type OrderId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

// Limit vs Market is the most fundamental order type distinction.
// Limit: "fill me at this price or better, otherwise rest in the book"
// Market: "fill me immediately at whatever price is available, never rest"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub order_type: OrderType,
    // For market orders this field is ignored during matching.
    // Convention: use u64::MAX for market buys and 0 for market sells
    // so the price check in submit_order always passes.
    pub price: Price,
    pub quantity: Quantity,
}

impl Order {
    // Convenience constructors so callers don't construct Order manually.
    pub fn limit(id: OrderId, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            id,
            side,
            order_type: OrderType::Limit,
            price,
            quantity,
        }
    }

    pub fn market(id: OrderId, side: Side, quantity: Quantity) -> Self {
        // Price sentinel: MAX for buys (matches any ask), 0 for sells (matches any bid).
        let price = match side {
            Side::Bid => u64::MAX,
            Side::Ask => 0,
        };
        Self {
            id,
            side,
            order_type: OrderType::Market,
            price,
            quantity,
        }
    }
}
