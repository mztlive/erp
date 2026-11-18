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

/// 为京东授权导入打开本地普通文件；`-` 表示从标准输入读取。两种来源都走
/// [`read_json`] 的字节上限和脱敏解析，错误不会带出路径或授权内容。
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

/// 将选定的授权输入流解析为调用方指定的 JSON 类型。最多接受 65,536 字节（含空白），
/// 超限时不尝试解析；读取、格式和字段错误只返回固定摘要，不暴露令牌值。
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
