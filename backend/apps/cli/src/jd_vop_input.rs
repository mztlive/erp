//! 京东授权文件输入；所有错误均为静态摘要，不携带文件内容。
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::de::DeserializeOwned;

/// 单份授权 JSON 的最大字节数，包含空白和所有字段。
pub const MAX_CREDENTIAL_INPUT_BYTES: u64 = 65_536;

/// 输入错误不保留底层错误，防止 JSON 值或路径被打印。
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("凭据输入读取失败；请检查本地文件或标准输入")]
    Read,
    #[error("凭据输入必须为本地普通文件；使用 --file - 读取标准输入")]
    NotRegularFile,
    #[error("凭据输入超过 65536 字节上限")]
    TooLarge,
    #[error("凭据 JSON 格式或字段无效；输入内容已隐藏")]
    InvalidJson,
}

/// 从本地普通文件或标准输入读取受限 JSON。
/// # 参数
/// `path` 为本地路径；`-` 表示标准输入，`T` 为拥有协议层的输入 DTO。
/// # 返回
/// 成功解析的输入，不打印内容或令牌。
/// # 错误
/// 文件无效、读取失败、输入超限或 JSON 无效时返回静态脱敏错误。
pub fn read_input<T: DeserializeOwned>(path: &Path) -> Result<T, InputError> {
    if path == Path::new("-") {
        return read_json(io::stdin().lock());
    }
    if !path.metadata().map_err(|_| InputError::Read)?.is_file() {
        return Err(InputError::NotRegularFile);
    }
    let file = File::open(path).map_err(|_| InputError::Read)?;
    if !file.metadata().map_err(|_| InputError::Read)?.is_file() {
        return Err(InputError::NotRegularFile);
    }
    read_json(file)
}

/// 读取有界字节并解析 JSON，禁止将底层反序列化错误带出输入边界。
/// # 参数
/// `reader` 为已选择的本地输入流，`T` 为协议输入类型。
/// # 返回
/// 合法且未超限的 JSON 对应值。
/// # 错误
/// 读取失败、超限及任何 JSON/字段错误均返回不包含输入的固定错误。
pub fn read_json<T: DeserializeOwned>(reader: impl Read) -> Result<T, InputError> {
    let mut bytes = Vec::new();
    reader.take(MAX_CREDENTIAL_INPUT_BYTES + 1).read_to_end(&mut bytes).map_err(|_| InputError::Read)?;
    if u64::try_from(bytes.len()).map_err(|_| InputError::TooLarge)? > MAX_CREDENTIAL_INPUT_BYTES {
        return Err(InputError::TooLarge);
    }
    serde_json::from_slice(&bytes).map_err(|_| InputError::InvalidJson)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestImport {
        access_token: String,
    }

    #[test]
    fn bounded_reader_accepts_json_and_exact_limit() {
        let valid = br#"{"access_token":"offline-test-secret"}"#;
        let input: TestImport = read_json(Cursor::new(valid)).unwrap();
        assert_eq!(input.access_token, "offline-test-secret");
        let mut bytes = valid.to_vec();
        bytes.resize(usize::try_from(MAX_CREDENTIAL_INPUT_BYTES).unwrap(), b' ');
        let input: TestImport = read_json(Cursor::new(bytes)).unwrap();
        assert_eq!(input.access_token, "offline-test-secret");
    }

    #[test]
    fn oversized_input_stops_after_the_first_excess_byte() {
        let bytes = vec![b' '; usize::try_from(MAX_CREDENTIAL_INPUT_BYTES + 100).unwrap()];
        let mut reader = Cursor::new(bytes);
        assert!(matches!(read_json::<TestImport>(&mut reader), Err(InputError::TooLarge)));
        assert_eq!(reader.position(), MAX_CREDENTIAL_INPUT_BYTES + 1);
    }

    #[test]
    fn json_errors_never_echo_values_unknown_fields_or_original_errors() {
        for bytes in [
            b"".as_slice(),
            br#"{"access_token":not-json-secret}"#.as_slice(),
            br#"{"access_token":"safe","secret-field":"hidden-value"}"#.as_slice(),
            br#"{"access_token":42}"#.as_slice(),
            br#"{"access_token":"one","access_token":"duplicate-secret"}"#.as_slice(),
            br#"{"access_token":"safe"} {"access_token":"trailing-secret"}"#.as_slice(),
            &[0xff],
        ] {
            let error = read_json::<TestImport>(Cursor::new(bytes)).err().unwrap();
            assert_eq!(error, InputError::InvalidJson);
            assert_eq!(error.to_string(), "凭据 JSON 格式或字段无效；输入内容已隐藏");
        }
    }

    #[test]
    fn reader_error_drops_untrusted_error_body() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("never-print-token-or-path"))
            }
        }
        let error = read_json::<TestImport>(FailingReader).err().unwrap();
        assert_eq!(error, InputError::Read);
        assert!(!error.to_string().contains("never-print-token"));
        assert!(!format!("{error:?}").contains("never-print-token"));
    }
}
