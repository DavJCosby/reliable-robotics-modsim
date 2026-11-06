mod args;

use crate::args::HydraulicModelCLIArgs;
use clap::Parser;
use simulator::{
    Simulator,
    clock::RealtimeClock,
    targets::{PrintlnTarget, UDPTarget},
};
use std::time::Duration;

#[tokio::main]
pub async fn main() {
    let args = HydraulicModelCLIArgs::parse();
    let events = args.events;

    let model = model::make_default_model(events).unwrap();
    let clock = RealtimeClock::new(Duration::from_millis(100));
    let print_target = PrintlnTarget;
    let udp_target = UDPTarget::new("127.0.0.1:0", "127.0.0.1:9000")
        .await
        .unwrap();

    let mut sim = Simulator::builder()
        .model(Box::new(model))
        .clock(Box::new(clock))
        .output_targets(vec![Box::new(print_target), Box::new(udp_target)])
        .sim_length(Duration::from_secs(60))
        .build();

    sim.run().await.unwrap();
}
