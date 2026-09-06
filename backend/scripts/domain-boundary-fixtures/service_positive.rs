use persistence_core::{Executor, NoTransaction};

pub struct CustomerService;

impl CustomerService {
    pub async fn customer_list(&self, executor: &mut dyn Executor) -> Result<(), String> {
        let _ = executor;
        let _ = NoTransaction;
        Ok(())
    }
}
