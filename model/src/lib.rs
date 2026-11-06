mod types;

use std::time::Duration;

pub use crate::types::*;
use simulator::Error;
use uom::si::{
    f64::{Pressure, Time, VolumeRate},
    pressure::bar,
    time::second,
    volume_rate::liter_per_second,
};

impl HydraulicsModel {
    pub fn snapshot(&self) -> HydraulicsOutputData {
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
            tank_pressures_bar: self
                .tanks
                .iter()
                .map(|tank| tank.pressure.get::<bar>())
                .collect(),
        }
    }

    fn apply_event(&mut self, event: ScheduledValveEvent) -> Result<(), Error> {
        match &mut self.pipes[event.pipe.0].control {
            ValveControl::Triggered(open) => *open = event.open,
            _ => {
                return Err(Error::Model(format!(
                    "Cannot manually trigger pipe {}: valve is not a triggered valve",
                    event.pipe.0
                )));
            }
        }

        Ok(())
    }

    fn handle_physics(&mut self, t: &Duration) -> Result<(), Error> {
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
}

impl simulator::Model for HydraulicsModel {
    type Output = HydraulicsOutputData;

    fn init(&mut self) -> Result<Self::Output, simulator::Error> {
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

        while let Some(event) = self.event_schedule.front() {
            if !event.at.is_zero() {
                break;
            }

            let event = self.event_schedule.pop_front().unwrap();
            self.apply_event(event)?;
        }

        Ok(self.snapshot())
    }

    fn step(&mut self, t: &std::time::Duration) -> Result<Self::Output, simulator::Error> {
        // time validation
        let previous = self
            .last_time
            .ok_or_else(|| Error::Model("step() called before init()".into()))?;

        if *t < previous {
            return Err(Error::Model("Simulation time moved backwards".into()));
        }

        // process every event due by this timestep (probably no more than 1 but doesn't hurt to be robust)
        while self
            .event_schedule
            .front()
            .is_some_and(|event| event.at <= *t)
        {
            let event = self.event_schedule.pop_front().unwrap();
            // what if we have an event in-between timestamps?
            // Split into two physics calculations/steps!
            // 1. old timestamp -> event timestamp
            // 2. event timestamp -> new timestamp
            self.handle_physics(&event.at)?;
            self.apply_event(event)?;
        }

        self.handle_physics(t)?;

        Ok(self.snapshot())
    }

    fn end(&mut self) -> Result<Self::Output, simulator::Error> {
        self.last_time
            .take()
            .ok_or_else(|| Error::Model("end() called before init() or after end()".to_string()))?;

        Ok(self.snapshot())
    }
}

pub fn make_default_model(events: Vec<ScheduledValveEvent>) -> Result<HydraulicsModel, Error> {
    let mut model_builder = HydraulicsModelBuilder::new();
    model_builder.set_bulk_modulus(Pressure::new::<bar>(200.0));

    // set up tanks
    let accumulator = model_builder.add_tank(Tank::new(10.0, 150.0));
    let cylinder = model_builder.add_tank(Tank::new(1.0, 1.0));
    let tank = model_builder.add_tank(Tank::new(1.0, 1.0));

    // set up pipes/valves
    let pipe_conductance = liters_per_second_per_bar(0.001);

    let _fill = model_builder.add_pipe(Pipe::new(
        accumulator,
        cylinder,
        pipe_conductance,
        ValveFlow::OneWay,
        ValveControl::Triggered(false),
    ));

    let _drain = model_builder.add_pipe(Pipe::new(
        cylinder,
        tank,
        pipe_conductance,
        ValveFlow::Bidirectional,
        ValveControl::Triggered(false),
    ));

    let max_pressure = Pressure::new::<bar>(100.0);
    let deadband = Pressure::new::<bar>(5.0);

    let _relief = model_builder.add_pipe(Pipe::new(
        cylinder,
        tank,
        pipe_conductance,
        ValveFlow::Bidirectional,
        ValveControl::PressureRelief {
            open_at: max_pressure,
            close_at: max_pressure - deadband,
            state: false,
        },
    ));

    // finalize

    model_builder.set_event_schedule(events)?;
    let model = model_builder.build()?;

    Ok(model)
}
