use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use yajrc::RpcError;

pub fn extract<T: DeserializeOwned>(value: &Value) -> Result<T, RpcError> {
    imbl_value::from_value(value.clone()).map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    })
}

pub fn combine(v1: Value, v2: Value) -> Result<Value, RpcError> {
    let (Value::Object(mut v1), Value::Object(v2)) = (v1, v2) else {
        return Err(RpcError {
            data: Some("params must be object".into()),
            ..yajrc::INVALID_PARAMS_ERROR
        });
    };
    for (key, value) in v2 {
        if v1.insert(key.clone(), value).is_some() {
            return Err(RpcError {
                data: Some(format!("duplicate key: {key}").into()),
                ..yajrc::INVALID_PARAMS_ERROR
            });
        }
    }
    Ok(Value::Object(v1))
}

pub fn invalid_params(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_PARAMS_ERROR
    }
}

pub fn invalid_request(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INVALID_REQUEST_ERROR
    }
}

pub fn parse_error(e: imbl_value::Error) -> RpcError {
    RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::PARSE_ERROR
    }
}

pub struct Flat<A, B>(pub A, pub B);
impl<'de, A, B> Deserialize<'de> for Flat<A, B>
where
    A: DeserializeOwned,
    B: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        let a = imbl_value::from_value(v.clone()).map_err(serde::de::Error::custom)?;
        let b = imbl_value::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Flat(a, b))
    }
}
