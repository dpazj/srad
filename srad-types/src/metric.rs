use std::sync::{atomic::AtomicBool, Arc};


#[derive(Clone)]
pub struct MetricValidToken(Arc<AtomicBool>);

#[derive(PartialEq, Debug)]
pub struct MetricValidTokenPtr(usize);

impl MetricValidToken {

  pub fn new() -> Self {
    MetricValidToken(Arc::new(AtomicBool::new(true))) 
  }

  pub fn invalidate(&self) {
    self.0.store(false, std::sync::atomic::Ordering::SeqCst);
  }

  pub fn is_valid(&self) -> bool {
    self.0.load( std::sync::atomic::Ordering::SeqCst)
  }

  pub fn token_ptr(&self) -> MetricValidTokenPtr {
    MetricValidTokenPtr(Arc::as_ptr(&self.0) as usize)
  }

}

#[cfg(test)]
mod tests {
  mod metric_valid_token {
    use crate::metric::MetricValidToken;


    #[test]
    fn test_token_ptr() {
      let tok1 = MetricValidToken::new();
      let tok1_cpy = tok1.clone();

      assert_eq!(tok1.token_ptr(), tok1_cpy.token_ptr());

      let tok2 = MetricValidToken::new();
      assert_ne!(tok1.token_ptr(), tok2.token_ptr());
    }

    #[test]
    fn test_invalidate() {
      let tok1 = MetricValidToken::new();
      let tok1_cpy = tok1.clone();
      assert!(tok1.is_valid());
      assert!(tok1_cpy.is_valid());

      tok1.invalidate();
      assert!(tok1.is_valid() == false);
      assert!(tok1_cpy.is_valid() == false);
    }
  }
}
