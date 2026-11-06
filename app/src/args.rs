use std::time::Duration;

use clap::Parser;
use model::{HydraulicsModel, PipeId, ValveEvent};
use simulator::{Event, ScheduledEvent};

#[derive(Parser)]
pub struct HydraulicModelCLIArgs {
    #[arg(long="event", value_parser = parse_event)]
    pub(crate) events: Vec<ScheduledEvent<Box<ValveEvent>>>,
}

impl HydraulicModelCLIArgs {
    pub(crate) fn into_events(self) -> Vec<ScheduledEvent<Box<dyn Event<HydraulicsModel>>>> {
        self.events
            .into_iter()
            .map(|scheduled| ScheduledEvent {
                at: scheduled.at,
                event: scheduled.event as Box<dyn Event<HydraulicsModel>>,
            })
            .collect()
    }
}

fn parse_event(input: &str) -> Result<ScheduledEvent<Box<ValveEvent>>, String> {
    let (time, command) = input.split_once(':').ok_or("Expected TIME:PIPE=STATE")?;

    let seconds = time
        .strip_suffix('s')
        .ok_or("Time must end in 's'")?
        .parse::<f64>()
        .map_err(|_| "Invalid event time")?;

    let at = Duration::try_from_secs_f64(seconds)
        .map_err(|_| "Event time must be finite and nonnegative")?;

    let (pipe, state) = command.split_once('=').ok_or("Expected PIPE=STATE")?;

    let pipe = pipe.parse::<usize>().map_err(|_| "Invalid pipe ID")?;
    let pipe = PipeId::from(pipe);

    let open = match state {
        "open" => true,
        "closed" => false,
        _ => return Err("State must be 'open' or 'closed'".into()),
    };

    Ok(ScheduledEvent {
        at,
        event: Box::new(ValveEvent { pipe, open }),
    })
}
