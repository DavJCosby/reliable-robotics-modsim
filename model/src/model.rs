use std::{ops::Div, time::Duration};

use crate::output_data::{HydraulicsOutputData, ValveState};
use simulator::Error;
use uom::si::{
    f64::{Pressure, Time, Volume, VolumeRate},
    pressure::bar,
    time::second,
    volume::liter,
    volume_rate::liter_per_second,
};

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

impl From<usize> for PipeId {
    fn from(value: usize) -> Self {
        Self(value)
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
}

pub struct HydraulicsModelBuilder {
    bulk_modulus: Option<Pressure>,
    tanks: Vec<Tank>,
    pipes: Vec<Pipe>,
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

    pub fn build(self) -> Result<HydraulicsModel, Error> {
        if let Some(bm) = self.bulk_modulus {
            Ok(HydraulicsModel {
                last_time: None,
                bulk_modulus: bm,
                tanks: self.tanks,
                pipes: self.pipes,
            })
        } else {
            Err(Error::Model(
                "Failed to provide a bulk modulus.".to_string(),
            ))
        }
    }
}

impl simulator::Model for HydraulicsModel {
    type Output = HydraulicsOutputData;

    fn snapshot(&self) -> Self::Output {
        HydraulicsOutputData {
            valve_states: self
                .pipes
                .iter()
                .map(|pipe| match &pipe.control {
                    ValveControl::Triggered(true) => ValveState::TriggeredOpen,
                    ValveControl::Triggered(false) => ValveState::TriggeredClose,
                    ValveControl::PressureRelief { state: true, .. } => ValveState::NaturallyOpen,
                    ValveControl::PressureRelief { state: false, .. } => {
                        ValveState::NaturallyClosed
                    }
                })
                .collect(),
        }
    }

    fn init(&mut self) -> Result<(), simulator::Error> {
        let bm = self.bulk_modulus.get::<bar>();
        if !bm.is_finite() || bm <= 0.0 {
            return Err(Error::Model(
                "Bulk modulus must be finite and positive.".to_string(),
            ));
        }

        for tank in &self.tanks {
            tank.validate()?;
        }

        for pipe in &self.pipes {
            pipe.validate(self.tanks.len())?;
        }

        self.last_time = Some(Duration::ZERO);

        Ok(())
    }

    fn step(&mut self, t: &std::time::Duration) -> Result<(), simulator::Error> {
        let previous = self
            .last_time
            .ok_or_else(|| Error::Model("step() called before init()".into()))?;

        let dt = t
            .checked_sub(previous)
            .ok_or_else(|| Error::Model("Simulation time moved backwards".into()))?
            .as_secs_f64();

        // can happen if we call after already handling physics for an event scheduled for right on the timestamp we're already at
        if dt == 0.0 {
            return Ok(());
        }

        let dt = Time::new::<second>(dt);

        // take a snapshot of current pressures, don't want pressure changes from early pipes in the loop to effect downstream calculations for this timestamp.
        let pressures: Vec<Pressure> = self.tanks.iter().map(|tank| tank.pressure).collect();
        let zero_flow = VolumeRate::new::<liter_per_second>(0.0);
        let mut net_inflow = vec![zero_flow; self.tanks.len()];

        for pipe in &mut self.pipes {
            let a = pipe.a.0;
            let b = pipe.b.0;

            // only modifies stuff if it's a pressure relief valve
            pipe.handle_pressure_relief(pressures[a]);

            if !pipe.is_open() {
                continue;
            }

            let mut q = pipe.conductance * (pressures[a] - pressures[b]);

            if let ValveFlow::OneWay = pipe.flow
                && q < zero_flow
            {
                q = zero_flow;
            }

            net_inflow[a] -= q;
            net_inflow[b] += q;
        }

        for (i, tank) in self.tanks.iter_mut().enumerate() {
            let p = pressures[i] + dt * self.bulk_modulus / tank.volume * net_inflow[i];

            if !p.is_finite() {
                return Err(Error::Model(format!("Non-finite pressure in tank {i}")));
            }

            tank.pressure = p;
        }

        self.last_time = Some(*t);

        Ok(())
    }

    fn end(&mut self) -> Result<(), simulator::Error> {
        self.last_time
            .take()
            .ok_or_else(|| Error::Model("end() called before init() or after end()".to_string()))?;

        Ok(())
    }
}
