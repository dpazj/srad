
pub struct ConnectionProperties {
  pub receive_maximum: Option<u16>,
  pub max_packet_size: Option<u32>
}

pub enum Transport {
  Tcp
}

pub struct MqttOptions {
  pub broker_addr: String,
  pub port: u16, 
  pub client_id: String,
  pub transport: Transport,
  pub credentials: Option<(String, String)>,
  pub connect_properties: Option<ConnectionProperties>
}

impl MqttOptions {

  pub fn new<S: Into<String>, S1: Into<String>>(client_id: S, addr: S1, port: u16) -> Self {
    Self {
      broker_addr: addr.into(),
      port,
      client_id: client_id.into(),
      transport: Transport::Tcp,
      credentials: None,
      connect_properties: None
    }
  }
}
