use std::collections::BTreeSet;

use entity_core::BaseModel;

use super::*;
use crate::{Error, Result};

impl SourceField {
    /// 返回界面字段名。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定中文名称。
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "商品名称",
            Self::Description => "商品介绍",
            Self::SkuName => "规格商品名称",
            Self::MainImage => "商品主图",
            Self::CarouselMedia => "商品轮播图",
            Self::DetailMedia => "商品详情图",
        }
    }
}

impl SourceValue {
    fn empty(&self) -> Self {
        match self {
            Self::Text(_) => Self::Text(None),
            Self::Images(_) => Self::Images(Vec::new()),
        }
    }

    /// 规范化文本边界空白，使来源基线与公司修订遵守相同规则。
    /// # 参数
    /// 无。
    /// # 返回
    /// 规范文本；空介绍归为未填写，图片顺序和身份保持原样。
    /// # 错误
    /// 无；名称非空在字段校验执行。
    pub fn normalized(&self) -> Self {
        match self {
            Self::Text(value) => {
                Self::Text(value.as_ref().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()))
            },
            Self::Images(_) => self.clone(),
        }
    }

    /// 比较展示内容身份；同一归档资产无需因预览URL变化形成冲突。
    /// # 参数
    /// `other` 为待比较的已知值。
    /// # 返回
    /// 具有相同可核验身份或相同文本时为 true。
    /// # 错误
    /// 无。
    pub fn equivalent(&self, other: &Self) -> bool {
        match (self,other) {
            (Self::Text(a),Self::Text(b))=>a==b,
            (Self::Images(a),Self::Images(b))=>a.len()==b.len() && a.iter().zip(b).all(|(a,b)| {
                if let (Some(a),Some(b))=(&a.file_asset_id,&b.file_asset_id) && a==b {return true;}
                matches!((&a.content_fingerprint,&b.content_fingerprint),(Some(a),Some(b)) if !a.is_empty() && a==b)
            }),
            _=>false
        }
    }
    /// 返回来源无法直接覆盖公司字段的明确原因。
    /// # 参数
    /// 目标展示字段。
    /// # 返回
    /// 容量超界或素材未核验的原因；可直接应用返回None。
    /// # 错误
    /// 无。
    pub fn application_blocker(&self, field: SourceField) -> Option<String> {
        self.company_capacity_blocker(field).or_else(|| {
            self.verification_required()
                .then(|| "供应商图片尚未核验或归档，请保留公司版本或先完成素材核验".into())
        })
    }

    fn company_capacity_blocker(&self, field: SourceField) -> Option<String> {
        let Self::Text(Some(value)) = self else { return None };
        let max = match field {
            SourceField::Name | SourceField::SkuName => 128,
            SourceField::Description => 512,
            _ => return None,
        };
        (value.trim().chars().count() > max).then(|| {
            format!(
                "供应商{}超过公司字段的{}字限制，请编辑整理后确认；完整来源资料已保留",
                field.label(),
                max
            )
        })
    }

    /// 判断图片来源是否缺少可核验内容或真实资产。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未具备采用条件时为 true。
    /// # 错误
    /// 无。
    pub fn verification_required(&self) -> bool {
        match self {
            Self::Text(_) => false,
            Self::Images(images) => images.iter().any(|i| {
                i.file_asset_id.as_deref().is_none_or(str::is_empty)
                    || i.content_fingerprint.as_deref().is_none_or(str::is_empty)
            }),
        }
    }

    /// 校验字段和值类型，防止将图片写入名称或清空名称。
    /// # 参数
    /// `field` 为目标字段。
    /// # 返回
    /// 合法时无返回值。
    /// # 错误
    /// 类型不符、空名称或过量图片时拒绝。
    pub fn validate(&self, field: SourceField) -> Result<()> {
        match (field, self) {
            (SourceField::Name | SourceField::SkuName, Self::Text(Some(v))) if !v.trim().is_empty() => Ok(()),
            (SourceField::Description, Self::Text(_)) => Ok(()),
            (SourceField::MainImage, Self::Images(v)) if v.len() <= 1 => Ok(()),
            (SourceField::CarouselMedia | SourceField::DetailMedia, Self::Images(v)) if v.len() <= 100 => {
                Ok(())
            },
            _ => Err(Error::ValidationError("商品资料字段类型或内容无效".into())),
        }
    }
}

impl ProductSource {
    /// 初始化唯一资料来源；公司已有不同内容时保留并承担人工责任。
    /// # 参数
    /// 商品与来源身份、当前公司内容及明确提供的观察字段。
    /// # 返回
    /// 未持久化的资料基线。
    /// # 错误
    /// 来源值不合法时拒绝。
    pub fn new(
        product: &str,
        supplier: &str,
        connection: &str,
        item: &str,
        current: &[(SourceField, Option<String>, SourceValue)],
        observation: SourceObservation,
    ) -> Result<Self> {
        let mut fields = Vec::new();
        for (field, sku_id, value) in observation.values {
            let value = value.normalized();
            value.validate(field)?;
            let company = current
                .iter()
                .find(|(f, s, _)| *f == field && *s == sku_id)
                .map(|(_, _, v)| v.clone())
                .unwrap_or_else(|| value.empty());
            fields.push(SourceFieldState {
                field,
                sku_id,
                baseline: value.clone(),
                baseline_confirmed: !value.verification_required(),
                latest: value.clone(),
                manually_maintained: !company.equivalent(&value),
                pending: value.verification_required() && !company.equivalent(&value),
                company,
            });
        }
        for (field, sku_id, value) in current {
            if !fields.iter().any(|s| s.field == *field && s.sku_id == *sku_id) {
                fields.push(SourceFieldState {
                    field: *field,
                    sku_id: sku_id.clone(),
                    baseline: value.clone(),
                    baseline_confirmed: false,
                    latest: value.clone(),
                    company: value.clone(),
                    manually_maintained: false,
                    pending: false,
                });
            }
        }
        let mut source = Self {
            base: BaseModel::new(product.into()),
            product_id: product.into(),
            supplier_id: supplier.into(),
            supplier_name: None,
            connection_id: connection.into(),
            catalog_item_id: item.into(),
            source_product_id: String::new(),
            sku_sources: Vec::new(),
            observed_at: observation.observed_at,
            fields,
            pending_count: 0,
            pending_fields: Vec::new(),
        };
        source.refresh_pending();
        Ok(source)
    }

    /// 在人工编辑入口登记全部字段责任，包括来源尚未提供的字段。
    /// # 参数
    /// 本次实际保存的完整公司展示资料。
    /// # 返回
    /// 新增字段和改变字段记为人工维护；空值编辑同样保留责任。
    /// # 错误
    /// 无。
    pub fn record_manual_edit(&mut self, current: &[(SourceField, Option<String>, SourceValue)]) {
        for (field, sku_id, value) in current {
            if !self.fields.iter().any(|s| s.field == *field && s.sku_id == *sku_id) {
                self.fields.push(SourceFieldState {
                    field: *field,
                    sku_id: sku_id.clone(),
                    baseline: value.clone(),
                    baseline_confirmed: false,
                    latest: value.clone(),
                    company: value.clone(),
                    manually_maintained: true,
                    pending: false,
                });
            }
        }
        self.record_company_edit(current);
    }

    /// 将真实人工编辑同步到字段责任，未变字段不受影响。
    /// # 参数
    /// `current` 为本次真实写入的公司资料。
    /// # 返回
    /// 更新字段缓存、责任与待确认摘要。
    /// # 错误
    /// 无。
    pub fn record_company_edit(&mut self, current: &[(SourceField, Option<String>, SourceValue)]) {
        for state in &mut self.fields {
            if let Some((_, _, value)) =
                current.iter().find(|(f, s, _)| *f == state.field && *s == state.sku_id)
            {
                if !state.company.equivalent(value) {
                    state.company = value.clone();
                    state.manually_maintained = true;
                }
                if state.company.equivalent(&state.latest) {
                    state.pending = false;
                    state.baseline = state.latest.clone();
                    state.baseline_confirmed = true;
                }
            }
        }
        self.refresh_pending();
    }

    /// 按字段应用来源更新，返回允许自动修改公司的字段。
    /// # 参数
    /// `current` 为真实公司资料，`observation` 为明确提供的来源字段。
    /// # 返回
    /// 应写入新商品修订的字段集合。
    /// # 错误
    /// 来源字段非法时拒绝。
    pub fn observe(
        &mut self,
        current: &[(SourceField, Option<String>, SourceValue)],
        observation: SourceObservation,
    ) -> Result<Vec<(SourceField, Option<String>, SourceValue)>> {
        self.record_company_edit(current);
        let mut changes = Vec::new();
        for (field, sku_id, value) in observation.values {
            let value = value.normalized();
            value.validate(field)?;
            if !self.fields.iter().any(|s| s.field == field && s.sku_id == sku_id) {
                if self.observe_new_field(field, sku_id.clone(), value.clone(), current) {
                    changes.push((field, sku_id, value));
                }
                continue;
            }
            let state = self
                .fields
                .iter_mut()
                .find(|s| s.field == field && s.sku_id == sku_id)
                .expect("来源字段已登记");
            if state.latest.equivalent(&value) {
                continue;
            }
            state.latest = value.clone();
            if state.company.equivalent(&value) {
                state.baseline = value;
                state.baseline_confirmed = true;
                state.pending = false;
                continue;
            }
            if state.baseline_confirmed && (state.baseline.equivalent(&value) || state.baseline == value) {
                state.pending = false;
                continue;
            }
            if state.manually_maintained || value.application_blocker(field).is_some() {
                state.pending = true;
                continue;
            }
            state.company = value.clone();
            state.baseline = value.clone();
            state.baseline_confirmed = true;
            state.pending = false;
            changes.push((field, sku_id, value));
        }
        self.observed_at = observation.observed_at;
        self.refresh_pending();
        Ok(changes)
    }

    fn observe_new_field(
        &mut self,
        field: SourceField,
        sku_id: Option<String>,
        value: SourceValue,
        current: &[(SourceField, Option<String>, SourceValue)],
    ) -> bool {
        let company = current
            .iter()
            .find(|(f, s, _)| *f == field && *s == sku_id)
            .map(|(_, _, v)| v.clone())
            .unwrap_or_else(|| value.empty());
        let manual = !company.equivalent(&value)
            && match &company {
                SourceValue::Text(v) => v.as_ref().is_some_and(|v| !v.is_empty()),
                SourceValue::Images(v) => !v.is_empty(),
            };
        let pending = !company.equivalent(&value) && (manual || value.application_blocker(field).is_some());
        let changed = !pending && !company.equivalent(&value);
        self.fields.push(SourceFieldState {
            field,
            sku_id,
            baseline: value.clone(),
            baseline_confirmed: !pending,
            latest: value.clone(),
            company: if pending { company } else { value },
            manually_maintained: manual,
            pending,
        });
        changed
    }

    /// 验证全部选择后处理差异，失败不产生部分规则变化。
    /// # 参数
    /// 已核对来源和公司版本的字段选择。
    /// # 返回
    /// 公司资料需要改变的字段。
    /// # 错误
    /// 字段重复、不在待处理范围、图片未核验或值非法时拒绝。
    pub fn resolve(
        &mut self,
        decisions: &[SourceDecision],
    ) -> Result<Vec<(SourceField, Option<String>, SourceValue)>> {
        self.validate_decisions(decisions)?;
        let mut changes = Vec::new();
        for decision in decisions {
            let edited = if decision.action == SourceDecisionAction::Edit {
                Some(self.canonical_edit_value(decision.value.as_ref().expect("编辑值已验证"))?)
            } else {
                None
            };
            let state = self
                .fields
                .iter_mut()
                .find(|s| s.field == decision.field && s.sku_id == decision.sku_id)
                .expect("选择已完整验证");
            let value = match decision.action {
                SourceDecisionAction::Keep => state.company.clone(),
                SourceDecisionAction::Adopt => state.latest.clone(),
                SourceDecisionAction::Edit => edited.expect("编辑值已规范化"),
            };
            if !value.equivalent(&state.company) {
                changes.push((state.field, state.sku_id.clone(), value.clone()));
                state.company = value;
            }
            state.baseline = state.latest.clone();
            state.baseline_confirmed = true;
            state.pending = false;
            state.manually_maintained = decision.action != SourceDecisionAction::Adopt;
        }
        self.refresh_pending();
        Ok(changes)
    }

    fn validate_decisions(&self, decisions: &[SourceDecision]) -> Result<()> {
        if decisions.is_empty() || decisions.len() > 500 {
            return Err(Error::ValidationError("请选择需要确认的差异字段".into()));
        }
        let mut seen = BTreeSet::new();
        for d in decisions {
            if !seen.insert((d.field, d.sku_id.clone())) {
                return Err(Error::ValidationError("差异字段不能重复".into()));
            }
            let state = self
                .fields
                .iter()
                .find(|s| s.field == d.field && s.sku_id == d.sku_id && s.pending)
                .ok_or_else(|| Error::ConflictError("差异已变化，请重新比较".into()))?;
            let value = match d.action {
                SourceDecisionAction::Keep => None,
                SourceDecisionAction::Adopt => Some(&state.latest),
                SourceDecisionAction::Edit => Some(
                    d.value
                        .as_ref()
                        .ok_or_else(|| Error::ValidationError("编辑后确认必须填写内容".into()))?,
                ),
            };
            if let Some(value) = value {
                value.validate(d.field)?;
                if let Some(reason) = value.company_capacity_blocker(d.field) {
                    return Err(Error::ValidationError(reason));
                }
                if d.action == SourceDecisionAction::Edit {
                    self.canonical_edit_value(value)?;
                }
                let missing_asset = matches!(value,SourceValue::Images(images) if images.iter().any(|i|i.file_asset_id.as_deref().is_none_or(str::is_empty)));
                if (d.action == SourceDecisionAction::Adopt && value.verification_required()) || missing_asset
                {
                    return Err(Error::BusinessLogicError(
                        "供应商图片尚未核验或归档，请保留公司版本或先完成素材核验".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 将编辑图片限制为本商品当前图片或已核验来源图片，并丢弃客户端伪造元数据。
    fn canonical_edit_value(&self, value: &SourceValue) -> Result<SourceValue> {
        let SourceValue::Images(images) = value else { return Ok(value.normalized()) };
        let mut result = Vec::new();
        for image in images {
            let id = image
                .file_asset_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Error::ValidationError("编辑图片必须是当前可选的已归档素材".into()))?;
            let candidate = self
                .fields
                .iter()
                .find_map(|field| {
                    for (value, source) in [(&field.company, false), (&field.latest, true)] {
                        if let SourceValue::Images(values) = value
                            && let Some(found) = values.iter().find(|i| {
                                i.file_asset_id.as_deref() == Some(id)
                                    && (!source
                                        || i.content_fingerprint.as_deref().is_some_and(|h| !h.is_empty()))
                            })
                        {
                            return Some(found.clone());
                        }
                    }
                    None
                })
                .ok_or_else(|| Error::Forbidden("仅可选用当前商品或本次已核验的供应商图片".into()))?;
            result.push(candidate);
        }
        Ok(SourceValue::Images(result))
    }

    fn refresh_pending(&mut self) {
        self.pending_fields = self.fields.iter().filter(|f| f.pending).map(|f| f.field).collect();
        self.pending_count = u32::try_from(self.pending_fields.len()).expect("字段数受来源规格上限约束");
    }

    /// 生成只含待确认字段的读取快照。
    /// # 参数
    /// 当前公司商品版本。
    /// # 返回
    /// 无成本资料的差异视图。
    /// # 错误
    /// 无。
    pub fn view(&self, product_version: u64) -> ProductSourceDiffView {
        ProductSourceDiffView {
            product_id: self.product_id.clone(),
            product_version,
            source_version: self.base.version,
            sku_versions: Default::default(),
            source: ProductSourceIdentity {
                supplier_id: self.supplier_id.clone(),
                supplier_name: self.supplier_name.clone(),
                connection_id: self.connection_id.clone(),
                catalog_item_id: self.catalog_item_id.clone(),
            },
            observed_at: self.observed_at,
            pending_count: self.pending_count,
            fields: self
                .fields
                .iter()
                .filter(|s| s.pending)
                .map(|s| SourceDiffField {
                    field: s.field,
                    sku_id: s.sku_id.clone(),
                    sku_no: None,
                    label: s.field.label().into(),
                    company: s.company.clone(),
                    supplier: s.latest.clone(),
                    baseline: s.baseline.clone(),
                    manually_maintained: s.manually_maintained,
                    verification_required: s.latest.application_blocker(s.field).is_some(),
                    application_blocker: s.latest.application_blocker(s.field),
                })
                .collect(),
        }
    }
}
