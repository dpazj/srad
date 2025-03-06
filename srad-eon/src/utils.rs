use std::time::{SystemTime, UNIX_EPOCH};

pub fn timestamp () -> u64
{
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64
}

#[derive(PartialEq)]
pub(crate) enum BirthType {
  Birth,
  Rebirth
}
