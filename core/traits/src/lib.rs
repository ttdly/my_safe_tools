use std::error::Error;

pub enum ResultCode {
    OK,
    TargetNotExist,
    UnknownError,
}

pub struct ExecuteResult {
    result_code: ResultCode,
    message: String,
}

impl ExecuteResult {
    pub fn ok() -> ExecuteResult {
        ExecuteResult {
            result_code: ResultCode::OK,
            message: String::from(""),
        }
    }

    pub fn unknown(error: Box<dyn Error>) -> ExecuteResult {
        ExecuteResult {
            result_code: ResultCode::UnknownError,
            message: error.to_string(),
        }
    }

    pub fn target_not_exist(message: &str) -> ExecuteResult {
        ExecuteResult {
            result_code: ResultCode::TargetNotExist,
            message: String::from(message),
        }
    }
}

pub trait Application {
    fn execute(&self) -> ExecuteResult;
}
