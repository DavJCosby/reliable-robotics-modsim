use std::{collections::VecDeque, ops::Div, time::Duration};

use serde::Serialize;
use simulator::{Error, OutputFrameData};
use uom::si::{
    f64::{Pressure, Volume, VolumeRate},
    pressure::bar,
    volume::liter,
    volume_rate::liter_per_second,
};

#[derive(Debug, Serialize)]
pub enum ValveState {
    NaturallyOpen,
    NaturallyClosed,
    TriggeredOpen,
    TriggeredClose,
}

#[derive(Debug, Serialize)]
pub struct HydraulicsOutputData {
    // for each pipe, the valve state and whether hardware control is needed
    pub valve_states: Vec<ValveState>,
    // tank order, in bar
    pub tank_pressures_bar: Vec<f64>,
}
impl OutputFrameData for HydraulicsOutputData {}

pub enum ValveFlow {
    OneWay,
    Bidirectional,
}

pub enum ValveControl {
    Triggered(bool),
    PressureRelief {
        open_at: Pressure,
        close_at: Pressure,
        state: bool,
    },
}

pub struct Tank {
    pub(crate) volume: Volume,
    pub(crate) pressure: Pressure,
}

impl Tank {
    pub fn new(vol_liters: f64, pressure_bars: f64) -> Self {
        Tank {
            volume: Volume::new::<liter>(vol_liters),
            pressure: Pressure::new::<bar>(pressure_bars),
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        let v = self.volume.get::<liter>();
        let p = self.pressure.get::<bar>();

        if !v.is_finite() || v <= 0.0 || !p.is_finite() {
            // I'd print out actual volume and pressure values, but I'm too lazy :)
            return Err(Error::Model(
                "Tank volume must be finite and positive; pressure must be finite.".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct TankId(pub(crate) usize);

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct PipeId(pub(crate) usize);

#[derive(Clone, Debug)]
pub struct ScheduledValveEvent {
    pub at: Duration,
    pub pipe: PipeId,
    pub open: bool,
}

impl std::str::FromStr for PipeId {
    type Err = std::num::ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

pub type Conductance = <VolumeRate as Div<Pressure>>::Output;

// helper for building conductance because it's a pain
pub fn liters_per_second_per_bar(value: f64) -> Conductance {
    VolumeRate::new::<liter_per_second>(value) / Pressure::new::<bar>(1.0)
}

pub struct Pipe {
    pub(crate) a: TankId,
    pub(crate) b: TankId,
    pub(crate) conductance: Conductance,
    pub(crate) flow: ValveFlow,
    pub(crate) control: ValveControl,
}

impl Pipe {
    pub fn new(
        a: TankId,
        b: TankId,
        conductance: Conductance,
        flow: ValveFlow,
        control: ValveControl,
    ) -> Self {
        Pipe {
            a,
            b,
            conductance,
            flow,
            control,
        }
    }

    pub fn handle_pressure_relief(&mut self, pressure: Pressure) {
        if let ValveControl::PressureRelief {
            open_at,
            close_at,
            state,
        } = &mut self.control
        {
            if pressure >= *open_at {
                *state = true;
            } else if pressure <= *close_at {
                *state = false;
            }
        }
    }

    pub fn is_open(&self) -> bool {
        match &self.control {
            ValveControl::Triggered(open) => *open,
            ValveControl::PressureRelief { state, .. } => *state,
        }
    }

    pub fn validate(&self, tank_count: usize) -> Result<(), Error> {
        let g = self.conductance;

        if self.a.0 >= tank_count || self.b.0 >= tank_count {
            return Err(Error::Model("Pipe endpoints are invalid".to_string()));
        }

        if !g.is_finite() || g < liters_per_second_per_bar(0.0) {
            return Err(Error::Model("Conductance is invalid".to_string()));
        }

        if let ValveControl::PressureRelief {
            open_at, close_at, ..
        } = &self.control
            && (!open_at.get::<bar>().is_finite()
                || !close_at.get::<bar>().is_finite()
                || close_at >= open_at)
        {
            return Err(Error::Model(
                "Relief thresholds must be finite, with close_at < open_at.".to_string(),
            ));
        }

        Ok(())
    }
}

pub struct HydraulicsModel {
    pub(crate) last_time: Option<Duration>,
    pub(crate) bulk_modulus: Pressure,
    pub(crate) tanks: Vec<Tank>,
    pub(crate) pipes: Vec<Pipe>,
    // easier to pop events off the front of the schedule if it's a double ended queue
    pub(crate) event_schedule: VecDeque<ScheduledValveEvent>,
}

pub struct HydraulicsModelBuilder {
    bulk_modulus: Option<Pressure>,
    tanks: Vec<Tank>,
    pipes: Vec<Pipe>,
    event_schedule: VecDeque<ScheduledValveEvent>,
}

impl Default for HydraulicsModelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HydraulicsModelBuilder {
    pub fn new() -> Self {
        Self {
            bulk_modulus: None,
            tanks: Vec::new(),
            pipes: Vec::new(),
            event_schedule: VecDeque::new(),
        }
    }

    pub fn set_bulk_modulus(&mut self, bm: Pressure) {
        self.bulk_modulus = Some(bm);
    }

    pub fn add_tank(&mut self, tank: Tank) -> TankId {
        self.tanks.push(tank);
        TankId(self.tanks.len() - 1)
    }

    pub fn add_pipe(&mut self, pipe: Pipe) -> Result<PipeId, Error> {
        let highest_tank_id = TankId(self.tanks.len() - 1);

        if pipe.a > highest_tank_id || pipe.b > highest_tank_id {
            return Err(Error::Model(
                "Attempt to connect a pipe to a tank that has not been added to the model yet."
                    .to_string(),
            ));
        }

        self.pipes.push(pipe);
        Ok(PipeId(self.pipes.len() - 1))
    }

    pub fn set_event_schedule(
        &mut self,
        mut events: Vec<ScheduledValveEvent>,
    ) -> Result<(), Error> {
        for event in &events {
            let pipe = self.pipes.get(event.pipe.0).ok_or_else(|| {
                Error::Model(format!(
                    "Scheduled event refers to invalid pipe {}",
                    event.pipe.0
                ))
            })?;

            match pipe.control {
                ValveControl::Triggered(_) => {}
                _ => {
                    return Err(Error::Model(format!(
                        "Cannot manually trigger pipe {}: valve is not a triggered valve",
                        event.pipe.0
                    )));
                }
            }
        }

        // keep input order for events at the same time
        events.sort_by_key(|event| event.at);
        self.event_schedule = events.into();

        Ok(())
    }

    pub fn build(self) -> Result<HydraulicsModel, Error> {
        if let Some(bm) = self.bulk_modulus {
            Ok(HydraulicsModel {
                last_time: None,
                bulk_modulus: bm,
                tanks: self.tanks,
                pipes: self.pipes,
                event_schedule: self.event_schedule,
            })
        } else {
            Err(Error::Model(
                "Failed to provide a bulk modulus.".to_string(),
            ))
        }
    }
}
