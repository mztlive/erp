use super::*;

fn text(value: &str) -> SourceValue {
    SourceValue::Text(Some(value.into()))
}
fn content(name: &str) -> Vec<(SourceField, Option<String>, SourceValue)> {
    vec![(SourceField::Name, None, text(name))]
}
fn observation(name: &str) -> SourceObservation {
    SourceObservation { values: content(name), observed_at: 20 }
}
fn source() -> ProductSource {
    ProductSource::new("p", "s", "c", "i", &content("原名"), observation("原名")).unwrap()
}
fn decision(field: SourceField, action: SourceDecisionAction) -> SourceDecision {
    SourceDecision { field, sku_id: None, action, value: None }
}
fn image(asset: Option<&str>, hash: Option<&str>, url: &str) -> SourceValue {
    SourceValue::Images(vec![SourceImage {
        url: Some(url.into()),
        file_asset_id: asset.map(Into::into),
        content_fingerprint: hash.map(Into::into),
    }])
}

#[test]
fn unedited_source_changes_auto_apply_only_changed_field() {
    let mut state = source();
    let changes = state.observe(&content("原名"), observation("新名")).unwrap();
    assert_eq!(changes, content("新名"));
    assert_eq!(state.pending_count, 0);
    assert!(!state.fields[0].manually_maintained);
}
#[test]
fn same_field_manual_edit_is_retained_until_decision() {
    let mut state = source();
    state.record_company_edit(&content("公司名"));
    assert!(state.observe(&content("公司名"), observation("供应商新名")).unwrap().is_empty());
    let diff = state.view(3);
    assert_eq!(diff.pending_count, 1);
    assert_eq!(diff.fields[0].company, text("公司名"));
    assert_eq!(diff.fields[0].supplier, text("供应商新名"));
    state.resolve(&[decision(SourceField::Name, SourceDecisionAction::Keep)]).unwrap();
    assert!(state.observe(&content("公司名"), observation("供应商新名")).unwrap().is_empty());
    assert_eq!(state.pending_count, 0);
    state.observe(&content("公司名"), observation("再次更新")).unwrap();
    assert_eq!(state.pending_count, 1);
}
#[test]
fn adoption_restores_automatic_sync_even_when_company_value_is_same() {
    let mut state = source();
    state.record_company_edit(&content("公司名"));
    state.observe(&content("公司名"), observation("来源名")).unwrap();
    let changes = state.resolve(&[decision(SourceField::Name, SourceDecisionAction::Adopt)]).unwrap();
    assert_eq!(changes, content("来源名"));
    assert!(!state.fields[0].manually_maintained);
    assert_eq!(state.observe(&content("来源名"), observation("来源再更新")).unwrap(), content("来源再更新"));
}
#[test]
fn edit_confirmation_remains_manual_and_duplicate_fields_are_atomic() {
    let mut state = source();
    state.record_company_edit(&content("公司名"));
    state.observe(&content("公司名"), observation("来源名")).unwrap();
    let before = state.fields.clone();
    let d = decision(SourceField::Name, SourceDecisionAction::Keep);
    assert!(state.resolve(&[d.clone(), d]).is_err());
    assert_eq!(state.fields, before);
    let d = SourceDecision {
        field: SourceField::Name,
        sku_id: None,
        action: SourceDecisionAction::Edit,
        value: Some(text("编辑结果")),
    };
    assert_eq!(state.resolve(&[d]).unwrap(), content("编辑结果"));
    assert!(state.fields[0].manually_maintained);
    assert_eq!(state.pending_count, 0);
}
#[test]
fn independent_field_updates_and_late_source_fields_are_not_lost() {
    let mut state = source();
    state.record_company_edit(&content("公司名"));
    let mut obs = observation("来源名");
    obs.values.push((SourceField::Description, None, text("新介绍")));
    let mut current = content("公司名");
    current.push((SourceField::Description, None, SourceValue::Text(None)));
    assert_eq!(state.observe(&current, obs).unwrap(), vec![(SourceField::Description, None, text("新介绍"))]);
    assert_eq!(state.pending_fields, vec![SourceField::Name]);
}
#[test]
fn missing_source_field_is_distinct_from_explicit_empty() {
    let mut state = source();
    let mut obs = observation("原名");
    obs.values.push((SourceField::Description, None, text("介绍")));
    state.observe(&content("原名"), obs).unwrap();
    let mut current = content("原名");
    current.push((SourceField::Description, None, text("介绍")));
    state.observe(&current, observation("原名")).unwrap();
    assert_eq!(
        state.fields.iter().find(|f| f.field == SourceField::Description).unwrap().latest,
        text("介绍")
    );
    let mut empty = observation("原名");
    empty.values.push((SourceField::Description, None, SourceValue::Text(None)));
    assert_eq!(
        state.observe(&current, empty).unwrap(),
        vec![(SourceField::Description, None, SourceValue::Text(None))]
    );
}
#[test]
fn image_content_and_order_use_fingerprints_not_url_or_new_asset_identity() {
    let a = image(Some("a"), Some("hash"), "https://one");
    let b = image(Some("b"), Some("hash"), "https://two");
    assert!(a.equivalent(&b));
    let changed = image(Some("c"), Some("new-hash"), "https://one");
    assert!(!a.equivalent(&changed));
    let SourceValue::Images(first) = a.clone() else { unreachable!() };
    let SourceValue::Images(second) = changed else { unreachable!() };
    let mut both = first.clone();
    both.extend(second.clone());
    let mut reversed = second;
    reversed.extend(first);
    assert!(!SourceValue::Images(both).equivalent(&SourceValue::Images(reversed)));
}
#[test]
fn unknown_image_can_be_kept_without_repeating_same_observation() {
    let mut state = source();
    let unknown = image(None, None, "https://source/image");
    let mut obs = observation("原名");
    obs.values.push((SourceField::DetailMedia, None, unknown.clone()));
    state.observe(&content("原名"), obs.clone()).unwrap();
    assert_eq!(state.pending_count, 1);
    assert!(state.resolve(&[decision(SourceField::DetailMedia, SourceDecisionAction::Adopt)]).is_err());
    state.resolve(&[decision(SourceField::DetailMedia, SourceDecisionAction::Keep)]).unwrap();
    state.observe(&content("原名"), obs).unwrap();
    assert_eq!(state.pending_count, 0);
}
#[test]
fn sku_main_images_have_independent_field_responsibility() {
    let mut state = source();
    let mut initial = observation("原名");
    let a = (SourceField::MainImage, Some("sku-a".into()), image(Some("a"), Some("ha"), "https://a"));
    let b = (SourceField::MainImage, Some("sku-b".into()), image(Some("b"), Some("hb"), "https://b"));
    initial.values.extend([a.clone(), b.clone()]);
    state.observe(&[a.clone(), b.clone()], initial).unwrap();
    let edited_a = (
        SourceField::MainImage,
        Some("sku-a".into()),
        image(Some("company-a"), Some("hc"), "https://company-a"),
    );
    let mut updated = observation("原名");
    updated.values.push((
        SourceField::MainImage,
        Some("sku-a".into()),
        image(Some("a2"), Some("ha2"), "https://a2"),
    ));
    let updated_b =
        (SourceField::MainImage, Some("sku-b".into()), image(Some("b2"), Some("hb2"), "https://b2"));
    updated.values.push(updated_b.clone());
    assert_eq!(state.observe(&[edited_a.clone(), b], updated).unwrap(), vec![updated_b]);
    let pending = state.view(2);
    assert_eq!(pending.pending_count, 1);
    assert_eq!(pending.fields[0].sku_id.as_deref(), Some("sku-a"));
    assert_eq!(pending.fields[0].company, edited_a.2);
}
#[test]
fn source_only_changes_after_edit_do_not_erase_company_responsibility() {
    let mut state = source();
    state.record_company_edit(&content("公司名"));
    assert_eq!(state.pending_count, 0);
    state.observe(&content("公司名"), observation("原名")).unwrap();
    assert_eq!(state.pending_count, 0);
    state.record_company_edit(&content("原名"));
    state.observe(&content("原名"), observation("更新名")).unwrap();
    assert_eq!(state.pending_count, 1);
}

#[test]
fn arbitrary_asset_and_forged_metadata_are_rejected_before_mutation() {
    let mut state = source();
    let original = image(Some("company-image"), None, "https://company");
    let supplier = image(Some("source-image"), Some("real-hash"), "https://source");
    let current = vec![(SourceField::DetailMedia, None, original.clone())];
    state.fields.push(SourceFieldState {
        field: SourceField::DetailMedia,
        sku_id: None,
        baseline: original.clone(),
        baseline_confirmed: true,
        latest: supplier,
        company: original,
        manually_maintained: true,
        pending: true,
    });
    state.record_company_edit(&current);
    let fields = state.fields.clone();
    let malicious = SourceDecision {
        field: SourceField::DetailMedia,
        sku_id: None,
        action: SourceDecisionAction::Edit,
        value: Some(image(Some("unrelated-secret-document"), Some("forged"), "https://attacker")),
    };
    assert!(matches!(state.resolve(&[malicious]), Err(crate::Error::Forbidden(_))));
    assert_eq!(state.fields, fields);
    let valid = SourceDecision {
        field: SourceField::DetailMedia,
        sku_id: None,
        action: SourceDecisionAction::Edit,
        value: Some(image(Some("source-image"), Some("forged"), "https://attacker")),
    };
    let changes = state.resolve(&[valid]).unwrap();
    assert_eq!(changes[0].2, image(Some("source-image"), Some("real-hash"), "https://source"));
}

mod command_tests {
    use async_trait::async_trait;

    use super::super::command::{self, ResolutionFacts, ResolutionStore};
    use super::*;
    use crate::{Error, Result};
    struct Store {
        allowed: bool,
        receipt: Option<SourceResolutionReceipt>,
        facts: Option<ResolutionFacts>,
        calls: Vec<&'static str>,
    }
    #[async_trait]
    impl ResolutionStore for Store {
        async fn authorize(&mut self) -> Result<()> {
            self.calls.push("authorize");
            if self.allowed { Ok(()) } else { Err(Error::Forbidden("denied".into())) }
        }
        async fn receipt(&mut self, _: &str) -> Result<Option<SourceResolutionReceipt>> {
            self.calls.push("receipt");
            Ok(self.receipt.clone())
        }
        async fn facts(&mut self) -> Result<ResolutionFacts> {
            self.calls.push("facts");
            Ok(self.facts.take().expect("只读取一次事实"))
        }
        async fn save(
            &mut self,
            source: ProductSource,
            changes: Vec<(SourceField, Option<String>, SourceValue)>,
            req: ResolveSourceDiffRequest,
        ) -> Result<ProductSourceDiffView> {
            self.calls.push("save");
            assert_eq!(source.pending_count, 0);
            assert!(changes.is_empty());
            assert_eq!(req.idempotency_key, "cmd");
            Ok(source.view(4))
        }
    }
    fn request() -> ResolveSourceDiffRequest {
        ResolveSourceDiffRequest {
            idempotency_key: "cmd".into(),
            expected_product_version: 3,
            expected_source_version: 1,
            expected_sku_versions: Default::default(),
            decisions: vec![decision(SourceField::Name, SourceDecisionAction::Keep)],
        }
    }
    fn store() -> Store {
        let mut source = source();
        source.record_company_edit(&content("人工名称"));
        source.observe(&content("人工名称"), observation("来源更新")).unwrap();
        let facts = ResolutionFacts {
            product_version: 3,
            source,
            current: CompanySourceContent {
                values: content("人工名称"),
                sku_versions: Default::default(),
                sku_numbers: Default::default(),
            },
        };
        Store { allowed: true, receipt: None, facts: Some(facts), calls: Vec::new() }
    }
    #[tokio::test]
    async fn permission_denial_stops_before_receipt_or_facts() {
        let mut store = store();
        store.allowed = false;
        assert!(command::resolve(&mut store, "p", request()).await.is_err());
        assert_eq!(store.calls, vec!["authorize"]);
    }
    #[tokio::test]
    async fn identical_replay_is_authorized_but_has_no_fact_read_or_write() {
        let mut store = store();
        let req = request();
        store.receipt = Some(SourceResolutionReceipt {
            base: entity_core::BaseModel::new("receipt".into()),
            product_id: "p".into(),
            request: req.clone(),
            product_version: 4,
            source_version: 2,
            result: source().view(4),
        });
        command::resolve(&mut store, "p", req).await.unwrap();
        assert_eq!(store.calls, vec!["authorize", "receipt"]);
    }
    #[tokio::test]
    async fn changed_replay_or_deleted_receipt_never_reexecutes() {
        for mode in 0..3 {
            let mut store = store();
            let mut req = request();
            let mut base = entity_core::BaseModel::new("receipt".into());
            if mode == 0 {
                req.decisions.clear();
            } else if mode == 1 {
                base.deleted_at = 1;
            }
            store.receipt = Some(SourceResolutionReceipt {
                base,
                product_id: if mode == 2 { "other".into() } else { "p".into() },
                request: req,
                product_version: 4,
                source_version: 2,
                result: source().view(4),
            });
            assert!(command::resolve(&mut store, "p", request()).await.is_err());
            assert_eq!(store.calls, vec!["authorize", "receipt"]);
        }
    }
    #[tokio::test]
    async fn either_company_or_source_version_change_rejects_before_write() {
        for source_change in [true, false] {
            let mut store = store();
            if source_change {
                store.facts.as_mut().unwrap().source.base.version += 1
            } else {
                store.facts.as_mut().unwrap().product_version += 1
            };
            assert!(command::resolve(&mut store, "p", request()).await.is_err());
            assert_eq!(store.calls, vec!["authorize", "receipt", "facts"]);
        }
    }
    #[tokio::test]
    async fn sku_version_change_rejects_before_write() {
        let mut store = store();
        let mut req = request();
        req.decisions[0].sku_id = Some("sku".into());
        req.expected_sku_versions.insert("sku".into(), 1);
        store.facts.as_mut().unwrap().current.sku_versions.insert("sku".into(), 2);
        assert!(command::resolve(&mut store, "p", req).await.is_err());
        assert_eq!(store.calls, vec!["authorize", "receipt", "facts"]);
    }
    #[tokio::test]
    async fn valid_confirmation_saves_only_after_all_checks() {
        let mut store = store();
        command::resolve(&mut store, "p", request()).await.unwrap();
        assert_eq!(store.calls, vec!["authorize", "receipt", "facts", "save"]);
    }
}

#[test]
fn source_text_normalizes_without_truncating_unicode_and_reports_company_capacity() {
    let value = text(&"中".repeat(128));
    assert!(value.validate(SourceField::Name).is_ok());
    assert!(value.application_blocker(SourceField::Name).is_none());
    assert!(text(&"中".repeat(129)).application_blocker(SourceField::Name).is_some());
    assert_eq!(SourceValue::Text(Some("   ".into())).normalized(), SourceValue::Text(None));
    let mut state = source();
    assert!(state.observe(&content("原名"), observation(" 原名 ")).unwrap().is_empty());
    assert_eq!(state.pending_count, 0);
}

#[test]
fn company_title_override_does_not_block_sku_sales_name_sync() {
    let sku_name = (SourceField::SkuName, Some("sku".into()), text("原名"));
    let mut current = content("原名");
    current.push(sku_name);
    let mut state = ProductSource::new(
        "p",
        "s",
        "c",
        "i",
        &current,
        SourceObservation { values: current.clone(), observed_at: 1 },
    )
    .unwrap();
    current[0].2 = text("公司精修名称");
    let updated_sku = (SourceField::SkuName, Some("sku".into()), text("来源新名"));
    let mut updated = observation("来源新名");
    updated.values.push(updated_sku.clone());
    assert_eq!(state.observe(&current, updated).unwrap(), vec![updated_sku]);
    assert_eq!(state.pending_count, 1);
    assert_eq!(state.pending_fields, vec![SourceField::Name]);
    assert_eq!(state.view(3).fields[0].company, text("公司精修名称"));
}

#[test]
fn newly_provided_unverified_images_remain_pending_until_a_real_decision() {
    let mut state = source();
    let mut obs = observation("原名");
    obs.values.push((SourceField::DetailMedia, None, image(None, None, "https://source/new")));
    for _ in 0..3 {
        state.observe(&content("原名"), obs.clone()).unwrap();
        assert_eq!(state.pending_count, 1);
        assert!(state.view(1).fields[0].verification_required);
    }
    state.resolve(&[decision(SourceField::DetailMedia, SourceDecisionAction::Keep)]).unwrap();
    state.observe(&content("原名"), obs).unwrap();
    assert_eq!(state.pending_count, 0);
}

#[test]
fn initially_missing_image_applies_after_verification_unless_manually_edited() {
    let empty = vec![(SourceField::DetailMedia, None, SourceValue::Images(vec![]))];
    let missing = (SourceField::DetailMedia, None, image(None, None, "https://source/a"));
    let mut state = ProductSource::new(
        "p",
        "s",
        "c",
        "i",
        &empty,
        SourceObservation { values: vec![missing], observed_at: 1 },
    )
    .unwrap();
    let verified = (SourceField::DetailMedia, None, image(Some("hash"), Some("asset"), "https://source/a"));
    assert_eq!(
        state.observe(&empty, SourceObservation { values: vec![verified.clone()], observed_at: 2 }).unwrap(),
        vec![verified.clone()]
    );

    let mut state = ProductSource::new(
        "p",
        "s",
        "c",
        "i",
        &empty,
        SourceObservation {
            values: vec![(SourceField::DetailMedia, None, image(None, None, "https://source/a"))],
            observed_at: 1,
        },
    )
    .unwrap();
    let edited =
        vec![(SourceField::DetailMedia, None, image(Some("other"), Some("manual"), "https://company/a"))];
    state.record_manual_edit(&edited);
    assert!(
        state
            .observe(&edited, SourceObservation { values: vec![verified], observed_at: 2 })
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.pending_count, 1);
}
#[test]
fn manually_editing_then_clearing_a_not_yet_provided_field_keeps_manual_responsibility() {
    let mut initial = content("原名");
    initial.push((SourceField::Description, None, SourceValue::Text(None)));
    let mut state = ProductSource::new("p", "s", "c", "i", &initial, observation("原名")).unwrap();
    let mut edited = initial.clone();
    edited[1].2 = text("公司曾编辑介绍");
    state.record_manual_edit(&edited);
    state.record_manual_edit(&initial);
    let mut obs = observation("原名");
    obs.values.push((SourceField::Description, None, text("供应商首次提供介绍")));
    assert!(state.observe(&initial, obs).unwrap().is_empty());
    assert_eq!(state.pending_count, 1);
    assert_eq!(state.view(1).fields[0].company, SourceValue::Text(None));
}
#[test]
fn long_source_description_is_preserved_without_blocking_other_field_updates() {
    let mut state = source();
    let raw = "完整来源介绍".repeat(200);
    let mut obs = observation("来源新名");
    obs.values.push((SourceField::Description, None, text(&raw)));
    assert_eq!(state.observe(&content("原名"), obs).unwrap(), content("来源新名"));
    let view = state.view(2);
    assert_eq!(view.pending_count, 1);
    assert_eq!(view.fields[0].supplier, text(&raw));
    assert!(view.fields[0].application_blocker.as_ref().unwrap().contains("512"));
    assert!(state.resolve(&[decision(SourceField::Description, SourceDecisionAction::Adopt)]).is_err());
    let edit = SourceDecision {
        field: SourceField::Description,
        sku_id: None,
        action: SourceDecisionAction::Edit,
        value: Some(text("公司整理介绍")),
    };
    assert_eq!(state.resolve(&[edit]).unwrap(), vec![(SourceField::Description, None, text("公司整理介绍"))]);
    assert_eq!(
        state.fields.iter().find(|f| f.field == SourceField::Description).unwrap().baseline,
        text(&raw)
    );
}
#[test]
fn initial_explicit_company_override_is_a_handled_baseline_including_cleared_description() {
    let mut current = content("公司展示名");
    current.push((SourceField::Description, None, SourceValue::Text(None)));
    let mut obs = observation("来源原名");
    obs.values.push((SourceField::Description, None, text("完整来源介绍")));
    let mut state = ProductSource::new("p", "s", "c", "i", &current, obs.clone()).unwrap();
    assert_eq!(state.pending_count, 0);
    state.observe(&current, obs).unwrap();
    assert_eq!(state.pending_count, 0);
    let mut changed = observation("来源原名");
    changed.values.push((SourceField::Description, None, text("来源更新介绍")));
    state.observe(&current, changed).unwrap();
    assert_eq!(state.pending_count, 1);
    assert!(state.view(1).fields[0].manually_maintained);
}
