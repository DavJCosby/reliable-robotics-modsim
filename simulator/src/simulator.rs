use std::time::Duration;

use bon::Builder;
use futures::future::join_all;
use tokio::sync::mpsc;

use crate::{
    OutputFrameType,
    interface::{Clock, Error, Model, OutputFrame, OutputFrameData, OutputTarget},
};

#[derive(Builder)]
pub struct Simulator<D: OutputFrameData> {
    model: Box<dyn Model<Output = D>>,
    clock: Box<dyn Clock>,
    sim_length: Duration,
    output_targets: Option<Vec<Box<dyn OutputTarget<D>>>>,
}

async fn send_frame<D: OutputFrameData>(
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

impl<D: OutputFrameData> Simulator<D> {
    pub async fn run(&mut self) -> Result<(), Error> {
        let (queue_transmitter, queue_receiver) = mpsc::channel(64);

        let output_targets = self
            .output_targets
            .take()
            .ok_or(Error::OutputAlreadyStarted)?;

        let output_worker = tokio::spawn(start_output_queue_worker(output_targets, queue_receiver));

        self.run_model(queue_transmitter).await?;

        output_worker
            .await
            .map_err(|err| Error::Target(format!("Output worker failed: {err}")))?;

        Ok(())
    }

    async fn run_model(
        &mut self,
        queue_transmitter: mpsc::Sender<OutputFrame<D>>,
    ) -> Result<(), Error> {
        // init
        let data = self.model.init()?;
        send_frame(
            &queue_transmitter,
            OutputFrameType::Init,
            Duration::ZERO,
            data,
        )
        .await?;

        self.clock.start()?;
        let mut t = Duration::ZERO;
        // simulation steps
        while t < self.sim_length {
            t = self.clock.tick().await?.min(self.sim_length);
            let data = self.model.step(&t)?;
            send_frame(&queue_transmitter, OutputFrameType::Step, t, data).await?;
        }

        // end
        self.clock.end()?;
        let data = self.model.end()?;
        send_frame(&queue_transmitter, OutputFrameType::End, t, data).await?;

        Ok(())
    }
}
