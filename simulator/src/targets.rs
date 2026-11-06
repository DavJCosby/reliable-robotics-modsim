use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::{OutputFrameData, interface::*};

pub struct PrintlnTarget;

#[async_trait]
impl<D: OutputFrameData> OutputTarget<D> for PrintlnTarget {
    async fn send(&mut self, data: &OutputFrame<D>) -> Result<(), Error> {
        let time = data.time.as_secs_f32();
        println!("{:.2} [{:?}] : {:?}", time, data.ty, data.data);
        Ok(())
    }
}

pub struct PrintlnTargetVerbose;

#[async_trait]
impl<D: OutputFrameData> OutputTarget<D> for PrintlnTargetVerbose {
    async fn send(&mut self, data: &OutputFrame<D>) -> Result<(), Error> {
        let time = data.time.as_secs_f32();
        println!("t={:.5}, event=[{:?}] : {:#?}", time, data.ty, data.data);
        Ok(())
    }
}

pub struct FileTarget {
    _path: String,
}

#[async_trait]
impl<D: OutputFrameData> OutputTarget<D> for FileTarget {
    async fn send(&mut self, _data: &OutputFrame<D>) -> Result<(), Error> {
        // append line to a file
        Ok(())
    }
}

pub struct UDPTarget {
    socket: UdpSocket,
    target_addr: String,
}

impl UDPTarget {
    pub async fn new(
        socket_addr: &str,
        target_addr: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(socket_addr).await?;

        Ok(UDPTarget {
            socket,
            target_addr: target_addr.to_string(),
        })
    }
}

#[async_trait]
impl<D: OutputFrameData> OutputTarget<D> for UDPTarget {
    async fn send(&mut self, data: &OutputFrame<D>) -> Result<(), Error> {
        let mut buffer = [0u8; 64];

        let bytes = postcard::to_slice(&data, &mut buffer).map_err(|e| {
            Error::Target(format!("UDPTarget, error serializing: {}", e).to_string())
        })?;

        self.socket
            .send_to(bytes, &self.target_addr)
            .await
            .map_err(|e| Error::Target(format!("UDPTarget, error sending: {}", e).to_string()))?;

        Ok(())
    }
}
