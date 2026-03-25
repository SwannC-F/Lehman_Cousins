use chrono::{TimeZone, Utc};
use csv::Reader;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::time::Instant;
use tracing::{info, Level};

use lehman_cousins_core::core::{
    events::MarketEvent,
    models::{Order, OrderBookUpdate, OrderStatus, OrderType, PriceLevel, Side},
};
use lehman_cousins_core::strategies::traits::Strategy;

/// Paper Trader simulating strict exchange conditions.
struct PaperTrader {
    cash: Decimal,
    position: Decimal,
    taker_fee_rate: Decimal, // e.g., 0.001 for 0.1%
}

impl PaperTrader {
    fn new(initial_cash: Decimal) -> Self {
        Self {
            cash: initial_cash,
            position: Decimal::ZERO,
            taker_fee_rate: dec!(0.001), // strictly 0.1% Taker Fees
        }
    }

    /// Simulate execution at the worst possible price (Bid for Sell, Ask for Buy).
    fn execute(&mut self, order: &Order, current_bid: Decimal, current_ask: Decimal) {
        let execution_price = match order.side {
            Side::Buy => current_ask, // Forced Slip: Buy at Ask
            Side::Sell => current_bid, // Forced Slip: Sell at Bid
        };

        let notional = execution_price * order.quantity;
        let fee = notional * self.taker_fee_rate;

        match order.side {
            Side::Buy => {
                self.cash -= notional + fee;
                self.position += order.quantity;
                tracing::debug!(
                    "PAPER BOUGHT {} @ {} (Fee: {})",
                    order.quantity, execution_price, fee
                );
            }
            Side::Sell => {
                self.cash += notional - fee;
                self.position -= order.quantity;
                tracing::debug!(
                    "PAPER SOLD {} @ {} (Fee: {})",
                    order.quantity, execution_price, fee
                );
            }
        }
    }

    fn calculate_mtm(&self, current_mid: Decimal) -> Decimal {
        self.cash + (self.position * current_mid)
    }
}

// A simple Dummy Strategy for backtesting purposes
struct DummyStatArb;

impl Strategy for DummyStatArb {
    fn name(&self) -> &str {
        "DummyStatArb"
    }

    fn on_start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_event(&mut self, event: &MarketEvent) -> Option<Vec<Order>> {
        match event {
            MarketEvent::OrderBook(ob) => {
                if ob.bids.is_empty() || ob.asks.is_empty() {
                    return None;
                }
                let bid = ob.bids[0].price;
                let ask = ob.asks[0].price;
                
                // Dummy logic for testing Paper PnL
                let spread = ask - bid;
                if spread < dec!(0.5) {
                    Some(vec![Order {
                        client_id: uuid::Uuid::new_v4(),
                        exchange_id: None,
                        symbol: ob.symbol.clone(),
                        side: Side::Buy,
                        order_type: OrderType::Market,
                        price: None,
                        quantity: dec!(0.5),
                        filled_quantity: dec!(0),
                        status: OrderStatus::Pending,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    }])
                } else if spread > dec!(2.0) {
                    Some(vec![Order {
                        client_id: uuid::Uuid::new_v4(),
                        exchange_id: None,
                        symbol: ob.symbol.clone(),
                        side: Side::Sell,
                        order_type: OrderType::Market,
                        price: None,
                        quantity: dec!(0.5),
                        filled_quantity: dec!(0),
                        status: OrderStatus::Pending,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    }])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Initializing Lehman Cousins Backtest Runner (O(N) CPU Bound)...");
    info!("MARKET CONDITIONS: 0.1% Taker Fees AND Strict Spread Crossing (Ask/Bid)");

    let mut strategy = DummyStatArb;
    strategy.on_start()?;

    let mut trader = PaperTrader::new(dec!(10000.0)); // $10k initial

    // Mock CSV format: timestamp_ms,bid,ask
    let csv_data = "\
timestamp_ms,bid,ask
1672531200000,65000.0,65002.0
1672531201000,65000.5,65001.0
1672531202000,64998.0,65002.5
1672531203000,65005.0,65005.1
1672531204000,65006.0,65008.0
1672531205000,65002.0,65002.2
";

    let mut reader = Reader::from_reader(csv_data.as_bytes());
    let mut total_events = 0;
    
    let start_time = Instant::now();
    let mut last_bid = dec!(0);
    let mut last_ask = dec!(0);

    for result in reader.records() {
        let record = result?;
        let ts_ms: i64 = record[0].parse()?;
        let bid: Decimal = record[1].parse()?;
        let ask: Decimal = record[2].parse()?;
        
        last_bid = bid;
        last_ask = ask;

        let timestamp = Utc.timestamp_millis_opt(ts_ms).unwrap();

        let event = MarketEvent::OrderBook(OrderBookUpdate {
            symbol: "BTCUSDT".to_string(),
            bids: vec![PriceLevel { price: bid, quantity: dec!(10.0) }],
            asks: vec![PriceLevel { price: ask, quantity: dec!(10.0) }],
            sequence: total_events,
            timestamp,
        });

        total_events += 1;

        if let Some(orders) = strategy.on_event(&event) {
            for order in orders {
                trader.execute(&order, bid, ask);
            }
        }
    }

    let elapsed = start_time.elapsed();
    strategy.on_stop()?;

    let mid = (last_bid + last_ask) / dec!(2.0);
    let final_mtm = trader.calculate_mtm(mid);

    info!(
        "Backtest complete. Processed {} events in {:?}",
        total_events, elapsed
    );
    info!(
        "PnL Report -> Initial: $10000.00 | Final Mark-to-Market: ${} | Position: {} BTC",
        final_mtm, trader.position
    );

    Ok(())
}
