use std::{collections::VecDeque, time::Duration};

use bon::Builder;
use futures::future::join_all;
use tokio::sync::mpsc;

use crate::{
    Event, OutputFrameType, ScheduledEvent,
    interface::{Clock, Error, Model, OutputFrame, OutputFrameData, OutputTarget},
};

#[derive(Builder)]
pub struct Simulator<M: Model> {
    model: M,
    #[builder(default)]
    events: Vec<ScheduledEvent<Box<dyn Event<M>>>>,
    clock: Box<dyn Clock>,
    sim_length: Duration,
    output_targets: Option<Vec<Box<dyn OutputTarget<M::Output>>>>,
}

async fn output_frame<D: OutputFrameData>(
    queue_transmitter: &mpsc::Sender<OutputFrame<D>>,
    ty: OutputFrameType,
    time: Duration,
    data: D,
) -> Result<(), Error> {
    queue_transmitter
        .send(OutputFrame {
            ty: Ok(ty),
            time,
            data,
        })
        .await
        .map_err(|_| Error::OutputQueueClosed)
}

async fn start_output_queue_worker<D: OutputFrameData>(
    mut targets: Vec<Box<dyn OutputTarget<D>>>,
    mut queue_receiver: mpsc::Receiver<OutputFrame<D>>,
) {
    while let Some(frame) = queue_receiver.recv().await {
        let futures: Vec<_> = targets.iter_mut().map(|item| item.send(&frame)).collect();

        for result in join_all(futures).await {
            if let Err(err) = result {
                eprintln!("Output failed: {err}");
            }
        }
    }
}

type EventQueue<M> = VecDeque<ScheduledEvent<Box<dyn Event<M> + 'static>>>;

impl<M: Model> Simulator<M> {
    pub async fn run(&mut self) -> Result<(), Error> {
        let (queue_transmitter, queue_receiver) = mpsc::channel(64);

        let output_targets = self
            .output_targets
            .take()
            .ok_or(Error::OutputAlreadyStarted)?;

        let output_worker = tokio::spawn(start_output_queue_worker(output_targets, queue_receiver));

        let run_result = self.run_model(queue_transmitter).await;

        let output_result = output_worker
            .await
            .map_err(|err| Error::Target(format!("Output worker failed: {err}")));

        run_result?;
        output_result
    }

    async fn run_model(
        &mut self,
        queue_transmitter: mpsc::Sender<OutputFrame<M::Output>>,
    ) -> Result<(), Error> {
        let mut t = Duration::ZERO;
        let error_catcher = async {
            // init simulation
            self.model.init()?;
            let mut events = self.init_events()?;

            let data = self.model.snapshot();
            output_frame(&queue_transmitter, OutputFrameType::Init, t, data).await?;

            // simulation steps
            self.clock.start()?;
            while t < self.sim_length {
                let target_t = self.clock.tick().await?;
                if target_t <= t {
                    return Err(Error::Clock(
                        "Clock must advance simulation time".to_string(),
                    ));
                }
                self.simulation_step(&mut events, &mut t, target_t)?;
                let data = self.model.snapshot();
                output_frame(&queue_transmitter, OutputFrameType::Step, t, data).await?;
            }

            // end simulation
            self.clock.end()?;
            self.model.end()?;
            let data = self.model.snapshot();
            output_frame(&queue_transmitter, OutputFrameType::End, t, data).await?;

            Ok(())
        }
        .await;

        if error_catcher.is_err() {
            // send 5 packets to minimize chance of it being lost.
            for _ in 0..5 {
                let data = self.model.snapshot();
                output_frame(&queue_transmitter, OutputFrameType::Abort, t, data)
                    .await
                    .unwrap();
            }
        }

        error_catcher
    }

    fn init_events(&mut self) -> Result<EventQueue<M>, Error> {
        for scheduled in &self.events {
            scheduled.event.validate(&self.model)?;
        }

        self.events.sort_by_key(|event| event.at);

        let mut events: VecDeque<_> = std::mem::take(&mut self.events).into();
        if events.front().is_some_and(|event| event.at.is_zero()) {
            let mut t = Duration::ZERO;
            self.simulation_step(&mut events, &mut t, Duration::ZERO)?;
        }
        Ok(events)
    }

    fn simulation_step(
        &mut self,
        events: &mut EventQueue<M>,
        current: &mut Duration,
        target: Duration,
    ) -> Result<(), Error> {
        // process every event due by this timestep (probably no more than 1 but doesn't hurt to be robust)
        while events.front().is_some_and(|event| event.at <= target) {
            let scheduled = events.pop_front().unwrap();
            // what if we have an event in-between timestamps?
            // Split into two physics calculations/steps!
            //  1. old timestamp -> event timestamp
            //  2. event timestamp -> new timestamp
            if scheduled.at > *current {
                self.model.step(&scheduled.at)?;
                *current = scheduled.at;
            }
            scheduled.event.apply(&mut self.model)?;
        }

        if target > *current {
            self.model.step(&target)?;
            *current = target;
        }
        Ok(())
    }
}
