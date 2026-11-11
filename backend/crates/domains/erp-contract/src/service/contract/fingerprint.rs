//! 合同分页版本的稳定业务事实指纹。

use crate::repository::scope::ContractVersion;

/// 编码合同身份和业务版本；输入顺序不影响分页指纹。
/// # 参数
/// versions 为当前查询的合同身份及业务版本。
/// # 返回
/// 按身份排序并带长度边界的稳定字节串。
/// # 错误
/// 无。
pub(super) fn version_fingerprint_input(versions: &[ContractVersion]) -> Vec<u8> {
    let mut pairs = versions.iter().map(|version| (version.id.as_str(), version.version)).collect::<Vec<_>>();
    pairs.sort_unstable();
    let mut out = Vec::new();
    push_u64(&mut out, u64::try_from(pairs.len()).expect("合同版本集合长度可表示为 u64"));
    for (id, version) in pairs {
        push_bytes(&mut out, id.as_bytes());
        push_u64(&mut out, version);
    }
    out
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_u64(out, u64::try_from(bytes.len()).expect("合同身份长度可表示为 u64"));
    out.extend_from_slice(bytes);
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// 对规范字节串计算稳定指纹（FNV-1a 64 位固定算法，不随工具链漂移）。
///
/// # 参数
/// * `input` - `version_fingerprint_input` 输出的规范字节串
///
/// # 返回
/// 返回 64 位指纹，仅用于同版本内快照比对。
///
/// # 错误
/// 不返回错误。
pub(super) fn stable_fingerprint(input: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::scope::ContractVersion;

    #[test]
    fn version_fingerprint_is_order_independent_and_tracks_business_changes() {
        let versions = [
            ContractVersion { id: "contract-b".into(), version: 2 },
            ContractVersion { id: "contract-a".into(), version: 1 },
        ];
        let reordered = [
            ContractVersion { id: "contract-a".into(), version: 1 },
            ContractVersion { id: "contract-b".into(), version: 2 },
        ];
        let changed = [
            ContractVersion { id: "contract-a".into(), version: 2 },
            ContractVersion { id: "contract-b".into(), version: 2 },
        ];
        let fingerprint = |rows: &[ContractVersion]| stable_fingerprint(&version_fingerprint_input(rows));
        assert_eq!(fingerprint(&versions), fingerprint(&reordered));
        assert_ne!(fingerprint(&versions), fingerprint(&changed));
        assert_ne!(fingerprint(&[]), fingerprint(&versions));
    }
}
