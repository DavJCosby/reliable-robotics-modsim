use simulator::{Error, Event};

use crate::model::{HydraulicsModel, PipeId, ValveControl};

#[derive(Clone, Debug)]
pub struct ValveEvent {
    pub pipe: PipeId,
    pub open: bool,
}

impl Event<HydraulicsModel> for ValveEvent {
    fn validate(&self, model: &HydraulicsModel) -> Result<(), Error> {
        let pipe = model.pipes.get(self.pipe.0).ok_or_else(|| {
            Error::Event(format!(
                "Scheduled event refers to invalid pipe {}",
                self.pipe.0
            ))
        })?;

        match pipe.control {
            ValveControl::Triggered(_) => Ok(()),
            _ => Err(Error::Event(format!(
                "Cannot manually trigger pipe {}: valve is not a triggered valve",
                self.pipe.0
            ))),
        }
    }

    fn apply(&self, model: &mut HydraulicsModel) -> Result<(), Error> {
        self.validate(model)?;
        if let ValveControl::Triggered(open) = &mut model.pipes[self.pipe.0].control {
            *open = self.open;
        }
        Ok(())
    }
}
