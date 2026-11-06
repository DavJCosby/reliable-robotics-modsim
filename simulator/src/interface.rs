use async_trait::async_trait;
use serde::Serialize;
use std::{fmt::Debug, time::Duration};
use thiserror::Error;

#[derive(Clone, Error, Debug, Serialize)]
pub enum Error {
    #[error("The model reported an error: {0}")]
    Model(String),

    #[error("An event reported an error: {0}")]
    Event(String),

    #[error("An output target reported an error: {0}")]
    Target(String),

    #[error("The simulation's clock reported an error: {0}")]
    Clock(String),

    // #[error("An error occurred during model initialization.")]
    // ModelInit,
    #[error(
        "Attempt to transfer ownership of OutputTargets to worker task a second time; implies we already started output."
    )]
    OutputAlreadyStarted,

    #[error("mpsc channel/queue for processing output target data closed unexpectedly.")]
    OutputQueueClosed,
}

pub trait Model {
    type Output: OutputFrameData;

    fn init(&mut self) -> Result<(), Error>;
    fn step(&mut self, t: &Duration) -> Result<(), Error>;
    // TODO: determine whether &mut self is truly necessary
    fn validate(&mut self, _t: &Duration) -> Result<(), Error> {
        println!("No validator provided, passes by default.");
        Ok(())
    }
    fn end(&mut self) -> Result<(), Error>;
    /// Must remain available before initialization and after a lifecycle or event error.
    fn snapshot(&self) -> Self::Output;
}

// events implementations are directly associated with a model, so we can make it generic over the model type
pub trait Event<M: Model> {
    fn validate(&self, model: &M) -> Result<(), Error>;
    fn apply(&self, model: &mut M) -> Result<(), Error>;
}

#[derive(Clone, Debug)]
pub struct ScheduledEvent<E> {
    pub at: Duration,
    pub event: E,
}

#[async_trait]
pub trait Clock {
    fn start(&mut self) -> Result<(), Error>;
    async fn tick(&mut self) -> Result<Duration, Error>;
    // fn elapsed(&mut self) -> Duration;
    fn end(&mut self) -> Result<(), Error>;
}

#[derive(Debug, Serialize)]
pub enum OutputFrameType {
    Init,
    Step,
    End,
    Abort,
}

pub trait OutputFrameData: Send + Sync + Debug + Serialize + 'static {}

#[derive(Debug, Serialize)]
pub struct OutputFrame<OutputData: OutputFrameData> {
    pub ty: Result<OutputFrameType, Error>,
    pub time: Duration,
    pub data: OutputData,
}

#[async_trait]
pub trait OutputTarget<OutputData: OutputFrameData>: Send + Sync {
    async fn send(&mut self, data: &OutputFrame<OutputData>) -> Result<(), Error>;
}
