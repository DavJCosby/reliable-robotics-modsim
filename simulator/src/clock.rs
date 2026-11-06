use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;

use crate::interface::Clock;

pub struct SimulatedClock {
    elapsed: Duration,
    timestep: Duration,
}

impl SimulatedClock {
    pub fn new(timestep: Duration) -> Self {
        SimulatedClock {
            elapsed: Duration::from_millis(0),
            timestep,
        }
    }
}

#[async_trait]
impl Clock for SimulatedClock {
    fn start(&mut self) -> Result<(), crate::Error> {
        // no-op
        Ok(())
    }

    async fn tick(&mut self) -> Result<Duration, crate::Error> {
        self.elapsed += self.timestep;

        Ok(self.elapsed)
    }

    fn end(&mut self) -> Result<(), crate::Error> {
        // no-op
        Ok(())
    }
}

pub struct RealtimeClock {
    start: Option<Instant>,
    last_tick: Option<Instant>,
    timestep: Duration,
}

impl RealtimeClock {
    pub fn new(timestep: Duration) -> Self {
        RealtimeClock {
            start: None,
            last_tick: None,
            timestep,
        }
    }
}

#[async_trait]
impl Clock for RealtimeClock {
    fn start(&mut self) -> Result<(), crate::Error> {
        let now = Instant::now();
        self.start = Some(now);
        self.last_tick = Some(now);
        Ok(())
    }

    async fn tick(&mut self) -> Result<Duration, crate::Error> {
        if let Some(start) = self.start
            && let Some(last_tick) = self.last_tick
        {
            let timestep = self.timestep;

            tokio::task::spawn_blocking(move || {
                while last_tick.elapsed() < timestep {
                    // oh this is neat, just learned about this
                    std::hint::spin_loop();
                }
            })
            .await
            .map_err(|err| crate::Error::Clock(err.to_string()))?;

            // this gradually accumulates error, no good
            // self.last_tick = Some(Instant::now());

            self.last_tick = Some(last_tick + timestep);

            return Ok(start.elapsed());
        } else {
            return Err(crate::Error::Clock(
                "Tick called before calling .start()".to_string(),
            ));
        }
    }

    fn end(&mut self) -> Result<(), crate::Error> {
        // no-op
        Ok(())
    }
}
