use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct DataValue {
    x: i32,
    y: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Data {
    key: i32,
    value: DataValue,
}
