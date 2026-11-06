use std::time::Duration;

use clap::Parser;
use model::{PipeId, ScheduledValveEvent};

#[derive(Parser, Debug)]
pub struct HydraulicModelCLIArgs {
    #[arg(long="event", value_parser = parse_event)]
    pub(crate) events: Vec<ScheduledValveEvent>,
}

fn parse_event(input: &str) -> Result<ScheduledValveEvent, String> {
    let (time, command) = input.split_once(':').ok_or("Expected TIME:PIPE=STATE")?;

    let seconds = time
        .strip_suffix('s')
        .ok_or("Time must end in 's'")?
        .parse::<f64>()
        .map_err(|_| "Invalid event time")?;

    let at = Duration::try_from_secs_f64(seconds)
        .map_err(|_| "Event time must be finite and nonnegative")?;

    let (pipe, state) = command.split_once('=').ok_or("Expected PIPE=STATE")?;

    let pipe = pipe.parse::<PipeId>().map_err(|_| "Invalid pipe ID")?;

    let open = match state {
        "open" => true,
        "closed" => false,
        _ => return Err("State must be 'open' or 'closed'".into()),
    };

    Ok(ScheduledValveEvent { at, pipe, open })
}
