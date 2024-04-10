use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(bound = "T: Serialize, for<'de2> T: Deserialize<'de2>")]
pub struct Request<T>
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    metadata: HashMap<String, String>,
    message: T,
}

impl<T> Request<T>
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    pub fn new(message: T) -> Self {
        Self {
            metadata: HashMap::new(),
            message,
        }
    }

    pub fn get_ref(&self) -> &T {
        &self.message
    }

    pub fn get_metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}
