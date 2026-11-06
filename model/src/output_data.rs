use serde::Serialize;
use simulator::OutputFrameData;

#[derive(Debug, Serialize)]
pub enum ValveState {
    NaturallyOpen,
    NaturallyClosed,
    TriggeredOpen,
    TriggeredClose,
}

#[derive(Debug, Serialize)]
pub struct HydraulicsOutputData {
    pub valve_states: Vec<ValveState>,
    // tank order, in bars
    pub tank_pressures_bar: Vec<f64>,
}
impl OutputFrameData for HydraulicsOutputData {}
