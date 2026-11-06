mod event;
mod model;
mod output_data;

pub use crate::{event::*, model::*, output_data::*};
use simulator::Error;
use uom::si::{f64::Pressure, pressure::bar};

pub fn make_default_model() -> Result<HydraulicsModel, Error> {
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

    let model = model_builder.build()?;

    Ok(model)
}
