//! 服务端候选票据绑定冻结地址；前端不得直接提交供应商四级编码。

use std::collections::BTreeSet;
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::address::{
    AddressCandidateQuery, AddressRequirement, ConfirmedSupplierAddress, SupplierAddressCandidate,
    SupplierAddressDepth, SupplierAddressLevel,
};
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::snapshot::ExecutionSnapshot;
use super::{SupplierApiOrderProcess, missing_capability};
use crate::{Error, Result};

/// 仅允许从上一层服务器票据查询下一级。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAddressQuery {
    pub level: SupplierAddressDepth,
    pub parent_ticket: Option<String>,
}

#[derive(Serialize)]
pub struct ExecutionAddressCandidate {
    pub level: SupplierAddressDepth,
    pub code: String,
    pub name: String,
    pub ticket: String,
}

#[derive(Serialize)]
pub struct ExecutionAddressCandidates {
    pub candidates: Vec<ExecutionAddressCandidate>,
}

/// 保存于密文票据内的来源全路径；不接受客户端明文拼装。
#[derive(Serialize, Deserialize)]
struct AddressTicket {
    purpose: String,
    execution_id: String,
    execution_version: u64,
    connection_id: String,
    purchase_revision_id: String,
    address_fingerprint: String,
    expires_at: Instant,
    path: Vec<SupplierAddressCandidate>,
}

impl AddressTicket {
    fn for_execution(
        execution: &ApiOrderExecution,
        path: Vec<SupplierAddressCandidate>,
        now: Instant,
    ) -> Result<Self> {
        Ok(Self {
            purpose: "supplier-execution-address-v1".into(),
            execution_id: execution.base.id.clone(),
            execution_version: execution.base.version,
            connection_id: execution.connection_id.clone(),
            purchase_revision_id: execution.purchase_order_revision_id.clone(),
            address_fingerprint: execution
                .address_fingerprint
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("冻结收货地址尚未建立，请等待系统准备".into()))?,
            expires_at: Instant::from_unix_secs(now.unix_secs().saturating_add(900)),
            path,
        })
    }

    fn validate(&self, execution: &ApiOrderExecution, now: Instant) -> Result<()> {
        if self.purpose != "supplier-execution-address-v1"
            || self.execution_id != execution.base.id
            || self.execution_version != execution.base.version
            || self.connection_id != execution.connection_id
            || self.purchase_revision_id != execution.purchase_order_revision_id
            || Some(self.address_fingerprint.as_str()) != execution.address_fingerprint.as_deref()
            || self.expires_at <= now
            || self.path.len() > 4
        {
            return Err(Error::ConflictError("地址候选已失效或不属于当前订单，请重新选择".into()));
        }
        for (index, candidate) in self.path.iter().enumerate() {
            let parent = index.checked_sub(1).map(|previous| self.path[previous].code.as_str());
            if depth_index(candidate.depth) != index
                || candidate.parent_code.as_deref() != parent
                || candidate.code.trim().is_empty()
                || candidate.name.trim().is_empty()
            {
                return Err(Error::ValidationError("供应商地址候选关系无效，请重新选择".into()));
            }
        }
        Ok(())
    }

    fn into_address(self) -> Result<ConfirmedSupplierAddress> {
        if self.path.len() != 4 {
            return Err(Error::ValidationError("请完整选择省、市、区县及街道".into()));
        }
        let level = |index: usize| SupplierAddressLevel {
            code: self.path[index].code.clone(),
            name: self.path[index].name.clone(),
        };
        Ok(ConfirmedSupplierAddress {
            connection_id: self.connection_id.clone(),
            snapshot_reference: self.execution_id.clone(),
            address_fingerprint: self.address_fingerprint.clone(),
            version: self.execution_version,
            province: level(0),
            city: level(1),
            county: level(2),
            town: level(3),
        })
    }
}

impl SupplierApiOrderProcess {
    /// 按已授权原执行返回服务器验证并封装的下一层地址候选。
    /// # 参数
    /// `id` 已由 Handler 执行 submit 对象范围授权，`query` 仅携带前一层票据。
    /// # 返回
    /// 当前连接和冻结地址专属的可选择候选，不执行下单。
    /// # 错误
    /// 资料不可编辑、票据失效、父级关系不符或外部读取失败时拒绝。
    pub async fn execution_address_candidates(
        &self,
        id: &str,
        query: ExecutionAddressQuery,
    ) -> Result<ExecutionAddressCandidates> {
        let execution = self.load(id).await?;
        ensure_address_editable(&execution, &self.decode(&execution)?)?;
        let connector = self.setup_connector(&execution).await?;
        if connector.address_requirement() != AddressRequirement::FourLevel {
            return Err(Error::ValidationError("当前供应商不使用四级地址选择".into()));
        }
        let path = self.parent_address_path(&execution, &query)?;
        let parent_code = path.last().map(|parent| parent.code.clone());
        let source = connector.address_source().ok_or_else(|| missing_capability("供应商地址候选查询"))?;
        let candidates = source
            .candidates(&AddressCandidateQuery { depth: query.level, parent_code: parent_code.clone() })
            .await
            .map_err(|error| Error::BusinessLogicError(error.summary))?
            .value;
        validate_candidates(&candidates, query.level, parent_code.as_deref())?;
        let mut result = Vec::new();
        for candidate in candidates {
            let mut selected = path.clone();
            selected.push(candidate.clone());
            let ticket = AddressTicket::for_execution(&execution, selected, Instant::now())?;
            let text =
                serde_json::to_string(&ticket).map_err(|_| Error::Internal("地址候选无法保存".into()))?;
            result.push(ExecutionAddressCandidate {
                level: candidate.depth,
                code: candidate.code,
                name: candidate.name,
                ticket: self.codec.encrypt(&text)?,
            });
        }
        Ok(ExecutionAddressCandidates { candidates: result })
    }

    pub(super) async fn setup_connector(
        &self,
        execution: &ApiOrderExecution,
    ) -> Result<Arc<dyn SupplierConnector>> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        self.runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))
    }

    pub(super) async fn apply_supplier_address(
        &self,
        execution: &mut ApiOrderExecution,
        snapshot: &mut ExecutionSnapshot,
        ticket: &str,
    ) -> Result<()> {
        ensure_address_editable(execution, snapshot)?;
        let proof = self.decode_address_ticket(ticket)?;
        proof.validate(execution, Instant::now())?;
        let address = proof.into_address()?;
        let connector = self.setup_connector(execution).await?;
        if connector.address_requirement() != AddressRequirement::FourLevel {
            return Err(Error::ValidationError("当前供应商不使用四级地址选择".into()));
        }
        let valid = connector
            .address_source()
            .ok_or_else(|| missing_capability("四级地址有效性核验"))?
            .validate(&address)
            .await
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if !valid.value {
            return Err(Error::BusinessLogicError("供应商不接受所选收货地址，请重新选择".into()));
        }
        snapshot.recipient.supplier_address = Some(address);
        snapshot.recipient.ensure_address_ready(
            AddressRequirement::FourLevel,
            &execution.connection_id,
            &execution.base.id,
            execution.address_fingerprint.as_deref().unwrap_or_default(),
        )?;
        execution.stage = ApiOrderStage::PrepareAddress;
        Ok(())
    }

    fn parent_address_path(
        &self,
        execution: &ApiOrderExecution,
        query: &ExecutionAddressQuery,
    ) -> Result<Vec<SupplierAddressCandidate>> {
        let path = if let Some(ticket) = &query.parent_ticket {
            let previous = self.decode_address_ticket(ticket)?;
            previous.validate(execution, Instant::now())?;
            previous.path
        } else {
            vec![]
        };
        if path.len() != depth_index(query.level) {
            return Err(Error::ValidationError("请按省、市、区县及街道逐级选择地址".into()));
        }
        Ok(path)
    }

    fn decode_address_ticket(&self, ticket: &str) -> Result<AddressTicket> {
        if ticket.len() > 16_384 {
            return Err(Error::ValidationError("地址候选无效，请重新选择".into()));
        }
        let plain = self
            .codec
            .decrypt(ticket)
            .map_err(|_| Error::ValidationError("地址候选无效，请重新选择".into()))?;
        serde_json::from_str(&plain).map_err(|_| Error::ValidationError("地址候选无效，请重新选择".into()))
    }
}

fn ensure_address_editable(execution: &ApiOrderExecution, snapshot: &ExecutionSnapshot) -> Result<()> {
    execution.ensure_unsent_for_purchase_change()?;
    if execution.stage != ApiOrderStage::AwaitingSetup || snapshot.context.is_some() {
        return Err(Error::ConflictError("供应商地址已准备或执行已推进，请刷新后核对".into()));
    }
    Ok(())
}

fn depth_index(depth: SupplierAddressDepth) -> usize {
    match depth {
        SupplierAddressDepth::Province => 0,
        SupplierAddressDepth::City => 1,
        SupplierAddressDepth::County => 2,
        SupplierAddressDepth::Town => 3,
    }
}

fn validate_candidates(
    candidates: &[SupplierAddressCandidate],
    depth: SupplierAddressDepth,
    parent: Option<&str>,
) -> Result<()> {
    let mut codes = BTreeSet::new();
    for candidate in candidates {
        if candidate.depth != depth
            || candidate.parent_code.as_deref() != parent
            || candidate.code.trim().is_empty()
            || candidate.name.trim().is_empty()
            || candidate.code.len() > 128
            || candidate.name.len() > 256
            || !codes.insert(&candidate.code)
        {
            return Err(Error::BusinessLogicError("供应商地址候选关系或内容异常，请联系管理员核实".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_party::SensitiveDataCodec;

    use super::*;

    fn execution() -> ApiOrderExecution {
        let mut value =
            ApiOrderExecution::queued("e".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        value.address_fingerprint = Some("address".into());
        value
    }

    fn path() -> Vec<SupplierAddressCandidate> {
        [
            SupplierAddressDepth::Province,
            SupplierAddressDepth::City,
            SupplierAddressDepth::County,
            SupplierAddressDepth::Town,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, depth)| SupplierAddressCandidate {
            depth,
            code: format!("{}", index + 1),
            name: format!("名称{index}"),
            parent_code: index.checked_sub(1).map(|parent| format!("{}", parent + 1)),
        })
        .collect()
    }

    #[test]
    fn ticket_binds_account_execution_version_address_and_expiry() {
        let execution = execution();
        let now = Instant::now();
        let ticket = AddressTicket::for_execution(&execution, path(), now).unwrap();
        ticket.validate(&execution, now).unwrap();
        let mut other = execution.clone();
        other.connection_id = "other".into();
        assert!(ticket.validate(&other, now).is_err());
        other = execution.clone();
        other.base.version += 1;
        assert!(ticket.validate(&other, now).is_err());
        other = execution.clone();
        other.address_fingerprint = Some("changed".into());
        assert!(ticket.validate(&other, now).is_err());
        assert!(ticket.validate(&execution, ticket.expires_at).is_err());
        let address = ticket.into_address().unwrap();
        address.validate_binding("c", "e", "address").unwrap();
    }

    #[test]
    fn incomplete_or_tampered_candidate_path_cannot_be_confirmed() {
        let execution = execution();
        let now = Instant::now();
        let mut selected = path();
        selected[2].parent_code = Some("wrong".into());
        assert!(
            AddressTicket::for_execution(&execution, selected, now)
                .unwrap()
                .validate(&execution, now)
                .is_err()
        );
        let ticket = AddressTicket::for_execution(&execution, path()[..3].to_vec(), now).unwrap();
        assert!(ticket.into_address().is_err());
        let codec = SensitiveDataCodec::from_secret(b"unit-test-only");
        let ticket = codec.encrypt("candidate proof").unwrap();
        assert_eq!(codec.decrypt(&ticket).unwrap(), "candidate proof");
        assert!(codec.decrypt(&(ticket + "changed")).is_err());
    }
}
