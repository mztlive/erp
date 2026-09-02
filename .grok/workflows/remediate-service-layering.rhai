let meta = #{
    name: "remediate-service-layering",
    description: "按 docs/service-layering-remediation-contract.md 分波关闭剩余 OPEN 责任簇：P0 优先、耦合 R+E 同批、同模块簇合并后不相交组并行改代码且不跑 cargo、对抗评审、resume 修评审、Quality 独占全仓门禁、失败则 resume 一轮再重跑门禁、失败停波、回写合同后每波提交一次",
    when_to_use: "当需要执行后端 Service 分层剩余整改合同时。发现新问题请用 sink-service-layering；本 workflow 只关闭合同第 6 节已编号且仍为 OPEN 的项，不实施 BLOCKED 项。Quality 两次门禁仍失败后不再开下一波。",
    phases: [
        #{ title: "Inventory", detail: "读取合同状态并与波次目录求交" },
        #{ title: "Implement", detail: "同模块簇合并为一次实现；不相交组并行改代码，不跑 cargo test/check/clippy" },
        #{ title: "Review", detail: "仅对实现成功的组合并行对抗评审" },
        #{ title: "Repair", detail: "resume 实现者，只修本波评审 blocker，不跑 cargo" },
        #{ title: "Quality", detail: "第 5 节统一门禁（唯一编译入口）、Mongo 证据；全绿才回写合同" },
        #{ title: "Gate-repair", detail: "门禁失败时 resume 原实现者或一个 integrator，只改代码，不跑 cargo" },
        #{ title: "Commit", detail: "Quality 通过且有关闭项时提交本波，失败不提交" },
        #{ title: "Report", detail: "写出本轮关闭证据和下波建议" },
    ],
};

let status_schema = #{
    "type": "object",
    "required": ["open_ids", "blocked_ids", "in_progress_ids", "done_count", "evidence"],
    "properties": #{
        "open_ids": #{ "type": "array", "maxItems": 220, "items": #{ "type": "string" } },
        "blocked_ids": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "in_progress_ids": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "done_count": #{ "type": "integer" },
        "evidence": #{ "type": "string" },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": [
        "cluster", "completed_ids", "skipped_ids", "files_changed",
        "checks_run", "needs_mongo", "mongo_ran", "summary", "blockers"
    ],
    "properties": #{
        "cluster": #{ "type": "string" },
        "completed_ids": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
        "skipped_ids": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "checks_run": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "needs_mongo": #{ "type": "boolean" },
        "mongo_ran": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "blockers": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let review_schema = #{
    "type": "object",
    "required": ["cluster", "passed", "issues", "summary"],
    "properties": #{
        "cluster": #{ "type": "string" },
        "passed": #{ "type": "boolean" },
        "issues": #{
            "type": "array",
            "maxItems": 24,
            "items": #{
                "type": "object",
                "required": ["file", "issue", "severity", "evidence", "fix"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "severity": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                    "fix": #{ "type": "string" },
                },
            },
        },
        "summary": #{ "type": "string" },
    },
};

let quality_schema = #{
    "type": "object",
    "required": [
        "passed", "closed_ids", "held_ids", "commands", "mongo_commands",
        "files_changed", "failed_paths", "remaining_issues", "environment_blocked", "summary"
    ],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "closed_ids": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "held_ids": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "mongo_commands": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "failed_paths": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "remaining_issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "environment_blocked": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
    },
};

let commit_schema = #{
    "type": "object",
    "required": ["committed", "commit_sha", "files_staged", "skipped_dirty", "summary"],
    "properties": #{
        "committed": #{ "type": "boolean" },
        "commit_sha": #{ "type": "string" },
        "files_staged": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "skipped_dirty": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

fn is_text(value) {
    type_of(value) == "string" && value != ""
}

fn is_text_array(value) {
    type_of(value) == "array"
}

fn is_true(value) {
    type_of(value) == "bool" && value == true
}

fn integer_or(value, fallback) {
    if type_of(value) == "i64" { value } else { fallback }
}

fn array_contains(items, wanted) {
    let found = false;
    if is_text_array(items) {
        for item in items {
            if item == wanted { found = true; }
        }
    }
    found
}

fn add_unique(items, value) {
    if is_text(value) {
        if !array_contains(items, value) { items.push(value); }
    }
    items
}

fn known_item_id(value) {
    let ok = false;
    if is_text(value) {
        if value.starts_with("APP-") || value.starts_with("MASTER-") || value.starts_with("SALES-") {
            ok = true;
        }
        if value.starts_with("PROC-") || value.starts_with("FUL-") || value.starts_with("FIN-") {
            ok = true;
        }
        if value.starts_with("INT-") || value.starts_with("CORE-") { ok = true; }
    }
    ok
}

fn join_text(items, sep) {
    let out = "";
    let first = true;
    if is_text_array(items) {
        for item in items {
            if is_text(item) {
                if !first { out += sep; }
                out += item;
                first = false;
            }
        }
    }
    out
}

fn agent_id_of(result) {
    let id = "";
    if result != () {
        if is_text(result.agent_id) { id = result.agent_id; }
    }
    id
}

fn remaining_budget() {
    let n = 999;
    let b = budget();
    if b != () { n = integer_or(b.remaining, 999); }
    n
}

fn conflict_prefix(prefix) {
    let key = "";
    if is_text(prefix) {
        if prefix != "backend/entities/src/" {
            if prefix != "backend/database/src/repository/" {
                if prefix != "backend/database/src/indexes/" {
                    key = prefix;
                }
            }
        }
    }
    key
}

fn prefixes_conflict(left, right) {
    let found = false;
    if is_text_array(left) {
        if is_text_array(right) {
            for pa in left {
                let ka = conflict_prefix(pa);
                if ka != "" {
                    for pb in right {
                        let kb = conflict_prefix(pb);
                        if kb != "" {
                            if ka == kb { found = true; }
                            if ka.starts_with(kb) { found = true; }
                            if kb.starts_with(ka) { found = true; }
                        }
                    }
                }
            }
        }
    }
    found
}

fn can_absorb(group, cluster) {
    let ok = false;
    let ids = [];
    for item_id in group.item_ids { ids = add_unique(ids, item_id); }
    for item_id in cluster.item_ids { ids = add_unique(ids, item_id); }
    if ids.len() <= 12 { ok = true; }
    ok
}

fn write_rounds_for(groups) {
    let rounds = [];
    for group in groups {
        let placed = false;
        let next = [];
        for round in rounds {
            let conflict = false;
            for existing in round {
                if prefixes_conflict(existing.prefixes, group.prefixes) { conflict = true; }
            }
            if !placed && !conflict {
                let copy = [];
                for existing in round { copy.push(existing); }
                copy.push(group);
                next.push(copy);
                placed = true;
            } else {
                next.push(round);
            }
        }
        if !placed {
            let solo = [];
            solo.push(group);
            next.push(solo);
        }
        rounds = next;
    }
    rounds
}

fn absorb_cluster(group, cluster) {
    let names = [];
    for name in group.member_names { names = add_unique(names, name); }
    names = add_unique(names, cluster.name);
    let ids = [];
    for item_id in group.item_ids { ids = add_unique(ids, item_id); }
    for item_id in cluster.item_ids { ids = add_unique(ids, item_id); }
    let prefixes = [];
    for prefix in group.prefixes { prefixes = add_unique(prefixes, prefix); }
    for prefix in cluster.prefixes { prefixes = add_unique(prefixes, prefix); }
    let needs_mongo = is_true(group.needs_mongo);
    if is_true(cluster.needs_mongo) { needs_mongo = true; }
    #{
        name: join_text(names, "+"),
        item_ids: ids,
        prefixes: prefixes,
        needs_mongo: needs_mongo,
        member_names: names,
    }
}

fn cluster_as_group(cluster) {
    #{
        name: cluster.name,
        item_ids: cluster.item_ids,
        prefixes: cluster.prefixes,
        needs_mongo: cluster.needs_mongo,
        member_names: [cluster.name],
    }
}

fn merge_overlapping_clusters(clusters) {
    let groups = [];
    for cluster in clusters {
        let placed = false;
        let next = [];
        for group in groups {
            if !placed && prefixes_conflict(group.prefixes, cluster.prefixes) && can_absorb(group, cluster) {
                next.push(absorb_cluster(group, cluster));
                placed = true;
            } else {
                next.push(group);
            }
        }
        if !placed { next.push(cluster_as_group(cluster)); }
        groups = next;
    }
    groups
}

fn is_generic_prefix(prefix) {
    let generic = false;
    if is_text(prefix) {
        if prefix == "backend/entities/src/" { generic = true; }
        if prefix == "backend/database/src/repository/" { generic = true; }
        if prefix == "backend/database/src/indexes/" { generic = true; }
        if prefix == "backend/services/src/" { generic = true; }
        if prefix == "backend/database/src/" { generic = true; }
        if prefix == "backend/crates/bpm/src/" { generic = true; }
    }
    generic
}

fn group_owns_path(prefixes, files_changed, path) {
    let owned = false;
    if is_text(path) {
        if is_text_array(files_changed) {
            for file_path in files_changed {
                if is_text(file_path) {
                    if path == file_path { owned = true; }
                    if path.starts_with(file_path) { owned = true; }
                    if file_path.starts_with(path) { owned = true; }
                }
            }
        }
        if !owned {
            if is_text_array(prefixes) {
                for prefix in prefixes {
                    if is_text(prefix) && !is_generic_prefix(prefix) {
                        if path == prefix { owned = true; }
                        if path.starts_with(prefix) { owned = true; }
                    }
                }
            }
        }
    }
    owned
}

fn fallback_impl(cluster) {
    #{
        cluster: cluster.name,
        success: false,
        completed_ids: [],
        skipped_ids: cluster.item_ids,
        needs_mongo: cluster.needs_mongo,
        mongo_ran: false,
        summary: "implementer failed",
        blockers: ["implementer returned unusable output"],
        files_changed: [],
    }
}

fn impl_from_agent(cluster, impl_result) {
    let out = fallback_impl(cluster);
    if impl_result != () && impl_result.success && impl_result.output != () {
        let completed = [];
        if is_text_array(impl_result.output.completed_ids) {
            completed = intersect_ids(impl_result.output.completed_ids, cluster.item_ids);
        }
        let files = [];
        if is_text_array(impl_result.output.files_changed) {
            files = impl_result.output.files_changed;
        }
        let skipped = [];
        if is_text_array(impl_result.output.skipped_ids) {
            skipped = impl_result.output.skipped_ids;
        }
        let blockers = [];
        if is_text_array(impl_result.output.blockers) {
            blockers = impl_result.output.blockers;
        }
        let summary = "implemented";
        if is_text(impl_result.output.summary) { summary = impl_result.output.summary; }
        out = #{
            cluster: cluster.name,
            success: true,
            completed_ids: completed,
            skipped_ids: skipped,
            needs_mongo: cluster.needs_mongo,
            mongo_ran: false,
            summary: summary,
            blockers: blockers,
            files_changed: files,
        };
    }
    out
}

fn quality_failed_paths(quality_output) {
    let paths = [];
    if quality_output != () {
        if is_text_array(quality_output.failed_paths) {
            for path in quality_output.failed_paths {
                paths = add_unique(paths, path);
            }
        }
        if is_text_array(quality_output.remaining_issues) {
            for issue in quality_output.remaining_issues {
                if is_text(issue) {
                    if issue.starts_with("backend/") { paths = add_unique(paths, issue); }
                }
            }
        }
    }
    paths
}

fn quality_issue_texts(quality_output) {
    let issues = [];
    if quality_output != () {
        if is_text_array(quality_output.remaining_issues) {
            for issue in quality_output.remaining_issues {
                issues = add_unique(issues, issue);
            }
        }
        if is_text(quality_output.summary) {
            issues = add_unique(issues, quality_output.summary);
        }
    }
    issues
}

fn no_cargo_rules_text() {
    let text = "";
    text += "CARGO: Do not run cargo test, cargo check, cargo clippy, cargo test --workspace, or clippy --workspace. ";
    text += "Do not run ./scripts/check-bpm-boundaries.sh, check-service-boundaries.sh, or check-permissions-drift.sh. ";
    text += "Do not set CARGO_TARGET_DIR. Do not use /tmp/erp-*-target, /tmp/cargo-*, or any extra target directory. ";
    text += "Disk cannot hold a second target. Quality is the only agent allowed to compile or test, using the workspace default target. ";
    text += "You may run rustfmt or cargo fmt on files you touched in this group, nothing else. ";
    text += "Do not run #[ignore] Mongo tests. Write the required tests as source only.\n";
    text
}

fn review_findings_payload(review) {
    let payload = "review unavailable; treat as failed";
    if review != () && review.success && review.output != () {
        if type_of(review.output.issues) == "array" {
            payload = json_encode(review.output.issues);
        } else {
            payload = json_encode(review.output);
        }
    }
    payload
}

fn cluster_of(name, item_ids, prefixes, needs_mongo) {
    #{
        name: name,
        item_ids: item_ids,
        prefixes: prefixes,
        needs_mongo: needs_mongo,
    }
}

fn wave_of(id, kind, title, clusters) {
    #{ id: id, kind: kind, title: title, clusters: clusters }
}

fn catalog_waves() {
    [
        wave_of("p0-sales-independent", "p0", "Sales P0: receivable delta and projection ACK", [
            cluster_of("sales-e11", ["SALES-E11"], ["backend/services/src/sales_review/", "backend/entities/src/"], false),
            cluster_of("sales-e15", ["SALES-E15"], ["backend/services/src/projection/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-sales-returns", "p0", "Sales P0: reversal plan and allocation sequence", [
            cluster_of("sales-reversal", ["SALES-E16", "SALES-E17"], ["backend/services/src/returns/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-proc-catalog-supplier", "p0", "Procurement P0: catalog facts and supplier profile", [
            cluster_of("proc-catalog", ["PROC-E01", "PROC-R01"], ["backend/services/src/procurement_responsibility/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("proc-eligibility", ["PROC-E03"], ["backend/services/src/supplier/", "backend/entities/src/"], false),
            cluster_of("proc-profile", ["PROC-E06", "PROC-E07", "PROC-E08"], ["backend/services/src/supplier/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-proc-offering-bpm", "p0", "Procurement P0: offering receipt and purchase approval plans", [
            cluster_of("proc-offering", ["PROC-E10"], ["backend/services/src/supplier_offering/", "backend/entities/src/"], false),
            cluster_of("proc-start-cancel", ["PROC-E11", "PROC-E12"], ["backend/services/src/purchase_order/", "backend/crates/bpm/src/"], false),
            cluster_of("proc-receipt", ["PROC-E13"], ["backend/services/src/purchase_order/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-proc-po-plan", "p0", "Procurement P0: coverage, creation basis, sourcing and amounts", [
            cluster_of("proc-coverage", ["PROC-E14", "PROC-R07"], ["backend/services/src/purchase_order/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("proc-basis", ["PROC-E15", "PROC-R08"], ["backend/services/src/purchase_order/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("proc-sourcing", ["PROC-E16"], ["backend/services/src/purchase_order/", "backend/entities/src/"], false),
            cluster_of("proc-draft-edit", ["PROC-E17"], ["backend/services/src/purchase_order/", "backend/entities/src/"], false),
            cluster_of("proc-amounts", ["PROC-E18"], ["backend/services/src/purchase_order/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-ful-core", "p0", "Fulfillment P0: receipt totals and acceptance rules", [
            cluster_of("ful-r01", ["FUL-R01"], ["backend/services/src/fulfillment/", "backend/database/src/repository/"], true),
            cluster_of("ful-e06", ["FUL-E06"], ["backend/services/src/fulfillment/", "backend/entities/src/"], false),
            cluster_of("ful-e07", ["FUL-E07"], ["backend/services/src/fulfillment/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-ful-supplier", "p0", "Fulfillment P0: supplier refund totals and settlement snapshot", [
            cluster_of("ful-refund", ["FUL-R03", "FUL-R05"], ["backend/services/src/supplier_fulfillment/", "backend/database/src/repository/"], true),
            cluster_of("ful-settlement", ["FUL-R06", "FUL-E12"], ["backend/services/src/supplier_settlement/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
        wave_of("p0-fin-cost-payable", "p0", "Finance P0: cost allocation and payable plans", [
            cluster_of("fin-cost", ["FIN-E01", "FIN-R01"], ["backend/services/src/cost/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-pay-plan", ["FIN-E02", "FIN-R05"], ["backend/services/src/payable/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-invoice-plan", ["FIN-E03", "FIN-R04"], ["backend/services/src/payable/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-recipient-evidence", ["FIN-E04", "FIN-E05"], ["backend/services/src/payable/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-fin-receivable-a", "p0", "Finance P0: receivable command, invoice plan, review decision", [
            cluster_of("fin-e07", ["FIN-E07"], ["backend/services/src/receivable/", "backend/entities/src/"], false),
            cluster_of("fin-e08", ["FIN-E08", "FIN-R10"], ["backend/services/src/receivable/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-e10", ["FIN-E10"], ["backend/services/src/receivable/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-fin-receivable-b", "p0", "Finance P0: W13 identity, review chain, ledger, receipts", [
            cluster_of("fin-e11", ["FIN-E11"], ["backend/services/src/receivable/", "backend/entities/src/"], false),
            cluster_of("fin-e12", ["FIN-E12", "FIN-R12"], ["backend/services/src/receivable/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-e13", ["FIN-E13", "FIN-R09"], ["backend/services/src/receivable/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("fin-e14", ["FIN-E14"], ["backend/services/src/receivable/", "backend/entities/src/"], false),
        ]),
        wave_of("p0-int-mall-as", "p0", "Integrations P0: mall fact DTO, cost share, after-sales limits", [
            cluster_of("int-e01", ["INT-E01"], ["backend/services/src/mall_order/", "backend/entities/src/"], false),
            cluster_of("int-cost", ["INT-R02", "INT-E05"], ["backend/services/src/mall_order/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-as-limit", ["INT-R11", "INT-R12"], ["backend/services/src/mall_after_sales/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
        wave_of("p0-int-sync-import", "p0", "Integrations P0: snapshot stale write, import apply, job terminal, evidence", [
            cluster_of("int-r16", ["INT-R16"], ["backend/services/src/mall_sync/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-apply", ["INT-R29", "INT-E25"], ["backend/services/src/legacy_import/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-job-terminal", ["INT-E27"], ["backend/services/src/legacy_import/", "backend/entities/src/bulk_job/"], false),
            cluster_of("int-e20", ["INT-E20"], ["backend/services/src/integration_ops/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-approval", "p1", "Approval remaining catalog, published graph and DTO", [
            cluster_of("app-catalog", ["APP-R03", "APP-S02"], ["backend/services/src/approval/", "backend/database/src/repository/bpm.rs"], true),
            cluster_of("app-published", ["APP-R04", "APP-E05"], ["backend/services/src/approval/", "backend/crates/bpm/src/", "backend/database/src/repository/bpm.rs"], true),
            cluster_of("app-identity", ["APP-E01"], ["backend/services/src/approval/", "backend/services/src/sales_order/", "backend/entities/src/"], false),
            cluster_of("app-dto", ["APP-E03", "APP-E07", "APP-R05"], ["backend/services/src/approval/", "backend/crates/bpm/src/"], false),
        ]),
        wave_of("p1-master", "p1", "Master remaining catalog, publication delivery and customer VO", [
            cluster_of("master-tree", ["MASTER-R01"], ["backend/services/src/catalog/", "backend/database/src/repository/", "backend/database/src/indexes/"], true),
            cluster_of("master-listing", ["MASTER-R02", "MASTER-E01", "MASTER-E02"], ["backend/services/src/catalog/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("master-delivery", ["MASTER-R04"], ["backend/services/src/publication/", "backend/database/src/repository/", "backend/database/src/indexes/"], true),
            cluster_of("master-customer", ["MASTER-E03", "MASTER-E04", "MASTER-E05", "MASTER-E06"], ["backend/services/src/customer/", "backend/services/src/party/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-sales", "p1", "Sales remaining close facts, refund lists and order/projection VO", [
            cluster_of("sales-close", ["SALES-R01", "SALES-E08"], ["backend/services/src/sales_order/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("sales-refund-io", ["SALES-R06", "SALES-R07"], ["backend/services/src/returns/", "backend/database/src/repository/"], true),
            cluster_of("sales-order-vo", ["SALES-E02", "SALES-E03", "SALES-E04", "SALES-E05"], ["backend/services/src/sales_order/", "backend/services/src/sales_review/", "backend/entities/src/"], false),
            cluster_of("sales-projection", ["SALES-E12", "SALES-E13", "SALES-E14"], ["backend/services/src/projection/", "backend/entities/src/"], false),
            cluster_of("sales-p2", ["SALES-E01", "SALES-E18"], ["backend/services/src/contract/", "backend/services/src/returns/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-proc-repo", "p1", "Procurement remaining list/detail read models and indexes", [
            cluster_of("proc-rule-list", ["PROC-R02"], ["backend/services/src/procurement_responsibility/", "backend/database/src/repository/"], true),
            cluster_of("proc-supplier-io", ["PROC-R03", "PROC-R04"], ["backend/services/src/supplier/", "backend/database/src/repository/"], true),
            cluster_of("proc-po-query", ["PROC-R05", "PROC-R06"], ["backend/services/src/purchase_order/", "backend/database/src/repository/"], true),
            cluster_of("proc-offering-list", ["PROC-R09"], ["backend/services/src/supplier_offering/", "backend/database/src/repository/"], true),
            cluster_of("proc-r10", ["PROC-R10"], ["backend/database/src/repository/supplier.rs", "backend/database/src/indexes/"], true),
        ]),
        wave_of("p1-proc-entity", "p1", "Procurement remaining identity, create factory and offering DTO", [
            cluster_of("proc-e02", ["PROC-E02"], ["backend/services/src/procurement_responsibility/", "backend/entities/src/"], false),
            cluster_of("proc-e05", ["PROC-E05"], ["backend/services/src/supplier/", "backend/entities/src/"], false),
            cluster_of("proc-e09", ["PROC-E09"], ["backend/services/src/supplier_offering/", "backend/entities/src/"], false),
            cluster_of("proc-e04", ["PROC-E04"], ["backend/services/src/supplier/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-ful", "p1", "Fulfillment remaining bundles, line factories and settlement reason", [
            cluster_of("ful-r04", ["FUL-R04"], ["backend/services/src/supplier_fulfillment/", "backend/database/src/repository/"], true),
            cluster_of("ful-r07", ["FUL-R07"], ["backend/services/src/supplier_settlement/", "backend/database/src/repository/"], true),
            cluster_of("ful-lines", ["FUL-E01", "FUL-E02", "FUL-E03"], ["backend/services/src/fulfillment/", "backend/entities/src/"], false),
            cluster_of("ful-drafts", ["FUL-E04", "FUL-E05", "FUL-E11"], ["backend/services/src/fulfillment/", "backend/services/src/inventory/", "backend/entities/src/"], false),
            cluster_of("ful-reason", ["FUL-E13", "FUL-R02"], ["backend/services/src/supplier_settlement/", "backend/services/src/supplier_fulfillment/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-fin", "p1", "Finance remaining list hydration, paging and task/snapshot VO", [
            cluster_of("fin-pay-list", ["FIN-R02", "FIN-R03", "FIN-R06", "FIN-R07"], ["backend/services/src/payable/", "backend/database/src/repository/"], true),
            cluster_of("fin-recv-scope", ["FIN-R08", "FIN-R11", "FIN-R13"], ["backend/services/src/receivable/", "backend/database/src/repository/"], true),
            cluster_of("fin-e06-e09", ["FIN-E06", "FIN-E09"], ["backend/services/src/payable/", "backend/services/src/receivable/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-int-mall", "p1", "Integrations remaining mall order batch reads and payment-plan VO", [
            cluster_of("int-mall-read", ["INT-R01", "INT-R03", "INT-R04", "INT-R05"], ["backend/services/src/mall_order/", "backend/database/src/repository/"], true),
            cluster_of("int-mall-plan", ["INT-R06", "INT-R07", "INT-E02", "INT-E03", "INT-E04"], ["backend/services/src/mall_order/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
        wave_of("p1-int-after-sales-backfill", "p1", "Integrations remaining after-sales paging and backfill", [
            cluster_of("int-as-page", ["INT-R08", "INT-R09", "INT-E06"], ["backend/services/src/mall_after_sales/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-as-write", ["INT-R10", "INT-R13", "INT-E07"], ["backend/services/src/mall_after_sales/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-backfill", ["INT-R14", "INT-R15", "INT-E08"], ["backend/services/src/mall_backfill/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
        wave_of("p1-int-sync", "p1", "Integrations remaining mall sync read models and mapping VO", [
            cluster_of("int-sync-page", ["INT-R17", "INT-R18"], ["backend/services/src/mall_sync/", "backend/database/src/repository/"], true),
            cluster_of("int-sync-batch", ["INT-R19", "INT-R20", "INT-R21", "INT-R22", "INT-R23", "INT-R24"], ["backend/services/src/mall_sync/", "backend/database/src/repository/"], true),
            cluster_of("int-sync-vo", ["INT-E09", "INT-E10", "INT-E11", "INT-E12", "INT-E13", "INT-E14", "INT-E15", "INT-E16"], ["backend/services/src/mall_sync/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-int-ops", "p1", "Integrations remaining ops uniqueness, evidence policy and decisions", [
            cluster_of("int-ops-repo", ["INT-R25", "INT-R26"], ["backend/services/src/integration_ops/", "backend/database/src/repository/"], true),
            cluster_of("int-ops-vo", ["INT-E17", "INT-E18", "INT-E19", "INT-E21", "INT-E22", "INT-E23"], ["backend/services/src/integration_ops/", "backend/entities/src/"], false),
        ]),
        wave_of("p1-int-import-supplier", "p1", "Integrations remaining import batch writes and supplier API DTO", [
            cluster_of("int-import", ["INT-R27", "INT-R28", "INT-R30", "INT-R31", "INT-E24", "INT-E26"], ["backend/services/src/legacy_import/", "backend/database/src/repository/", "backend/entities/src/"], true),
            cluster_of("int-supplier-api", ["INT-R32", "INT-E28", "INT-E29", "INT-E30"], ["backend/services/src/supplier_api/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
        wave_of("blocked-returns-occupancy", "blocked", "Blocked refund occupancy policy plus coupled SALES-E19", [
            cluster_of("sales-occupancy", ["SALES-R02", "SALES-R03", "SALES-R04", "SALES-R05", "SALES-E19"], ["backend/services/src/returns/", "backend/database/src/repository/", "backend/entities/src/"], true),
        ]),
    ]
}

fn catalog_item_ids(waves) {
    let ids = [];
    for wave in waves {
        for cluster in wave.clusters {
            for item_id in cluster.item_ids {
                ids = add_unique(ids, item_id);
            }
        }
    }
    ids
}

fn ids_in_wave(wave) {
    let ids = [];
    for cluster in wave.clusters {
        for item_id in cluster.item_ids { ids = add_unique(ids, item_id); }
    }
    ids
}

fn intersect_ids(left, right) {
    let out = [];
    if is_text_array(left) {
        for item_id in left {
            if array_contains(right, item_id) { out.push(item_id); }
        }
    }
    out
}

fn subtract_ids(left, right) {
    let out = [];
    if is_text_array(left) {
        for item_id in left {
            if !array_contains(right, item_id) { out.push(item_id); }
        }
    }
    out
}

fn executable_open(open_ids, blocked_ids) {
    subtract_ids(open_ids, blocked_ids)
}

fn wave_open_count(wave, open_ids) {
    intersect_ids(ids_in_wave(wave), open_ids).len()
}

fn any_kind_open(waves, kind, open_ids) {
    let found = false;
    for wave in waves {
        if wave.kind == kind {
            if wave_open_count(wave, open_ids) > 0 { found = true; }
        }
    }
    found
}

fn find_wave(waves, wave_id) {
    let found = ();
    for wave in waves {
        if found == () && wave.id == wave_id { found = wave; }
    }
    found
}

fn layering_rules_text() {
    let text = "";
    text += "CONTRACT: docs/service-layering-remediation-contract.md is the only status source.\n";
    text += "Read the exact ID rows, forced adjustment, close checks, and section 2/5/7 before editing.\n";
    text += "Repository owns persistence filters, projections, batch/exact reads, DB paging/sort/aggregation, index fit, ";
    text += "and atomic write primitives under the caller executor. It must take &mut dyn Executor as the final parameter, ";
    text += "must not open/commit transactions, must not grow a _with_session dual API, and must not return services DTO, HTTP View, or auth conclusions.\n";
    text += "Entity/VO/DTO own I/O-free invariants, state, conservation, identity, and deterministic construction. ";
    text += "No MongoDB, HTTP, global clock, global ID generator, or raw secrets. HMAC/crypto stays in Service/crypto port; pass typed results only.\n";
    text += "BPM owns ERP-agnostic process graphs and neutral command plans. No DocumentType, RBAC, DataScope, WorkItem, or MongoDB.\n";
    text += "Service keeps use-case orchestration, transactions, RBAC/DataScope, cross-aggregate decisions, gateways, audit/outbox, query params, and View mapping.\n";
    text += "Closing an item requires deleting the old Service helper/rule source. A one-line forwarder is not done.\n";
    text += "Query changes must not alter soft-delete, current pointer, business date, stable sort, first error, missing facts, money precision, or transaction visibility.\n";
    text += "Money, stock, task uniqueness, idempotency and version problems need concurrency proof; a snapshot query is not a lock.\n";
    text += "Follow section 7.2 per cluster: failing or characterizing tests first; domain; repository; service switch + delete old path; Mongo/integration if applicable.\n";
    text += "Section 7.3: one cluster = one business fact or tightly coupled R+E+S. Do not mix unrelated P0 with file splits.\n";
    text += "SALES-E19 must ship with SALES-R02..R05. Those four are BLOCKED until finance owners sign occupancy policy; do not invent that policy.\n";
    text += "Frozen files in backend/AGENTS.md (entities/src/lib.rs, database repository/mod.rs, indexes/mod.rs, services/src/lib.rs, errors.rs, page.rs, query.rs, etc.) ";
    text += "need a separate chore/erp-p0-amend-* foundation change. Skip the cluster and report a blocker instead of editing them in a domain batch.\n";
    text += "New methods, public or private, need multi-line Rust docs covering purpose, params, return, errors, and constraints.\n";
    text += "New behavior must land in a responsibility-named file, not appended to a giant mod.rs/service.rs/repository.rs.\n";
    text += "Never git reset, checkout, restore, clean, or discard unrelated dirty work. Do not change public HTTP shapes unless the row requires it.\n";
    text += "Do not git add or git commit. Do not edit docs/service-layering-remediation-contract.md. Do not mark rows DONE or IN_PROGRESS. Quality writes status after gates.\n";
    text
}

fn implement_prompt_for(wave, cluster, batch_id, dirty_note) {
    let prompt = "";
    prompt += "Implement only this service-layering group in the current parent workspace. Do not use an isolated worktree.\n";
    prompt += "Wave: " + wave.id + " (" + wave.kind + ") " + wave.title + ".\n";
    prompt += "Group: " + cluster.name + ". Member clusters: " + json_encode(cluster.member_names) + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ".\n";
    prompt += "Allowed prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Batch id to record: " + batch_id + ". Owner: Grok.\n";
    prompt += dirty_note + "\n";
    prompt += layering_rules_text();
    prompt += no_cargo_rules_text();
    prompt += "Grep only the assigned ID rows in docs/service-layering-remediation-contract.md. Never read that whole file. Then grep/read_file the live source those rows cite. Do not trust this prompt's memory of line numbers.\n";
    prompt += "If a row is already DONE or the cited problem is already gone, put that ID in skipped_ids with a skip reason. Do not re-implement it.\n";
    prompt += "Inspect git status first. Modify only service files under the prefixes plus the matching domain-owned backend/entities, backend/database repository/indexes, or backend/crates/bpm modules those rows name.\n";
    prompt += "Implement every assigned ID in this group. Shared modules in the group are intentional; do not skip an ID because another ID in the same group touches the same crate.\n";
    prompt += "Write the unit tests each row requires (failing or characterizing tests first per section 7.2) but do not execute them. Quality will run cargo test once for the whole wave.\n";
    prompt += "Return completed_ids only for items whose old rule source is deleted and whose required unit tests exist in source. Otherwise put them in skipped_ids with blockers.\n";
    prompt += "checks_run may list rustfmt on touched files; leave it empty if you formatted in the editor. mongo_ran must be false.\n";
    prompt
}

fn review_prompt_for(wave, cluster, impl_payload) {
    let prompt = "";
    prompt += "Adversarially review the current git diff for service-layering group " + cluster.name + " in wave " + wave.id + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Implementer payload is a hypothesis only: " + impl_payload + "\n";
    prompt += "Grep only the listed ID rows in docs/service-layering-remediation-contract.md; do not read the whole contract. Inspect the live diff with git diff, grep, and read_file. Do not approve from names.\n";
    prompt += "Fail closed: passed=true only with no concrete blocker.\n";
    prompt += "Must catch: leftover Service helpers; repository business rules or transactions; entity I/O/secrets/clock/id-generator; BPM taking ERP types; ";
    prompt += "missing tests for the row's applicable dimensions; missing method docs; edits to frozen files; public API/schema changes not required by the row; ";
    prompt += "behavior drift in soft-delete, current pointer, money, sort, or first-error; marking DONE in the contract.\n";
    prompt += "Severity blocker|major|minor. Every issue needs file, evidence, and a specific fix. Empty issues is valid only after reading the diff.\n";
    prompt
}

fn repair_prompt_for(wave, cluster, findings_payload, dirty_note, resumed) {
    let prompt = "";
    prompt += "Repair only the review blockers for group " + cluster.name + " in wave " + wave.id + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Review findings (fix these, do not expand scope): " + findings_payload + "\n";
    prompt += dirty_note + "\n";
    if resumed {
        prompt += "This session already implemented this group. Do not re-read the whole contract or rediscover the crate. ";
        prompt += "Use git diff plus the findings list.\n";
    } else {
        prompt += layering_rules_text();
        prompt += "Grep only the assigned ID rows; never read the whole contract file.\n";
    }
    prompt += no_cargo_rules_text();
    prompt += "Do not expand scope. Do not run cargo test/check/clippy or workspace gates. Quality will compile after this repair.\n";
    prompt += "Do not edit docs/service-layering-remediation-contract.md. Do not git add or git commit.\n";
    prompt
}

fn quality_prompt_for(wave, batch_id, cluster_payloads, review_payloads, mongo_needed, is_retry, previous_issues) {
    let prompt = "";
    prompt += "Run the unified quality gates for service-layering wave " + wave.id + ". You are the gate runner, not the implementer.\n";
    prompt += "Batch id: " + batch_id + ". Owner: Grok.\n";
    prompt += "Inspect git status and diff first. Never git reset/checkout/restore/clean. Preserve unrelated dirty files.\n";
    prompt += "Implementer payloads: " + cluster_payloads + "\n";
    prompt += "Review payloads: " + review_payloads + "\n";
    if is_retry {
        prompt += "This is the one allowed retry after a code-only gate repair. Previous remaining issues: " + previous_issues + "\n";
        prompt += "Re-run the gates against the current tree. Do not assume the previous cargo output is still valid.\n";
    }
    prompt += "Do NOT fix compile errors, test failures, behavior regressions, leftover Service helpers, or missing tests. ";
    prompt += "Those belong to a later resume of the original implementer. You may rustfmt only files from this wave that fail fmt --check.\n";
    prompt += "Keep repository/entity/service/BPM boundaries from contract section 2. Do not expand scope.\n";
    prompt += "Run from backend, in this order, and stop fixing — record failures instead:\n";
    prompt += "cargo fmt --all -- --check\n";
    prompt += "cargo check --workspace --all-features --locked\n";
    prompt += "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings\n";
    prompt += "cargo test --workspace --all-features --locked\n";
    prompt += "./scripts/check-bpm-boundaries.sh\n";
    prompt += "./scripts/check-service-boundaries.sh\n";
    prompt += "./scripts/check-permissions-drift.sh\n";
    prompt += "git diff --check\n";
    prompt += "All cargo commands must use the workspace default target directory only. ";
    prompt += "Do not set CARGO_TARGET_DIR. Do not use /tmp/erp-*-target, /tmp/cargo-*, or any other temporary or dedicated target dir. ";
    prompt += "Do not copy historical contract commands that exported CARGO_TARGET_DIR. Do not create or leave behind a temporary cargo target.\n";
    prompt += "If mongo_needed=" + mongo_needed + ", also run the new or named #[ignore] integration/concurrency/explain tests on an isolated replica set. ";
    prompt += "Use ERP_TEST_MONGO_URI if set, otherwise backend/scripts/dev-mongo.sh or docker compose --profile test, never a shared/production URI. ";
    prompt += "If Mongo cannot be started, set environment_blocked=true, do NOT mark those R/CAS/index items DONE, and put them in held_ids.\n";
    prompt += "If any command fails, set passed=false, do not edit the contract, fill failed_paths with the rustc/test file paths, ";
    prompt += "and put the command output summaries in remaining_issues. closed_ids must be empty when passed=false.\n";
    prompt += "Only after every gate passes, set passed=true and update docs/service-layering-remediation-contract.md for closed_ids: status DONE, 执行登记 with batch/owner/deps/evidence, ";
    prompt += "and append a 批次验收登记 covering layering result, targeted tests, Mongo N/A-or-results, unified gates, compat/rollback, and unrun external bounds.\n";
    prompt += "Do not mark BLOCKED rows DONE. Do not close SALES-E19 without SALES-R02..R05. Do not invent occupancy policy.\n";
    prompt += "closed_ids must be a subset of this wave's attempted IDs. held_ids are attempted but not closed.\n";
    prompt += "Do not git add or git commit. Leave this wave's code and contract edits in the working tree. A later commit agent will create one commit if this wave passed with closed_ids.\n";
    prompt
}

fn gate_repair_prompt_for(wave, cluster, findings_payload, dirty_note, resumed) {
    let prompt = "";
    prompt += "Repair only the Quality cargo/gate failures for group " + cluster.name + " in wave " + wave.id + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Quality failures (fix these, do not expand scope): " + findings_payload + "\n";
    prompt += dirty_note + "\n";
    if resumed {
        prompt += "This session already implemented this group. Use git diff plus the cargo/test log. Do not rediscover the crate.\n";
    } else {
        prompt += layering_rules_text();
        prompt += "Grep only the assigned ID rows; never read the whole contract file.\n";
    }
    prompt += no_cargo_rules_text();
    prompt += "Do not run cargo test/check/clippy. Quality will re-run the unified gates after this repair.\n";
    prompt += "Do not edit docs/service-layering-remediation-contract.md. Do not git add or git commit.\n";
    prompt
}

fn integrator_prompt_for(wave, findings_payload, dirty_note, prefixes) {
    let prompt = "";
    prompt += "Repair unattributed Quality cargo/gate failures for service-layering wave " + wave.id + ".\n";
    prompt += "These paths/issues could not be assigned to a single implementer group.\n";
    prompt += "Allowed prefixes: " + json_encode(prefixes) + ".\n";
    prompt += "Quality failures (fix these, do not expand scope): " + findings_payload + "\n";
    prompt += dirty_note + "\n";
    prompt += layering_rules_text();
    prompt += no_cargo_rules_text();
    prompt += "Only edit files implicated by the cargo/test log. Do not close extra IDs. Do not touch frozen files.\n";
    prompt += "Do not run cargo test/check/clippy. Quality will re-run the unified gates after this repair.\n";
    prompt += "Do not edit docs/service-layering-remediation-contract.md. Do not git add or git commit.\n";
    prompt += "Return completed_ids empty unless you fully finished an assigned wave ID; otherwise use skipped_ids/blockers.\n";
    prompt
}

fn commit_prompt_for(wave, batch_id, closed_ids, quality_payload) {
    let prompt = "";
    prompt += "Create exactly one git commit for service-layering wave " + wave.id + ".\n";
    prompt += "Batch id: " + batch_id + ". Closed IDs: " + json_encode(closed_ids) + ".\n";
    prompt += "Quality payload: " + quality_payload + "\n";
    prompt += "Inspect git status and diff first. Commit only this wave's files: Quality files_changed plus docs/service-layering-remediation-contract.md if this wave updated it, plus matching tests.\n";
    prompt += "Do not stage unrelated dirty or untracked files. Record any left-unstaged paths in skipped_dirty.\n";
    prompt += "Never git reset, checkout, restore, clean, stash, rebase, merge, or push. Never amend a commit you did not just create in this step.\n";
    prompt += "If this wave has no staged changes after the scoped add, set committed=false and commit_sha=none. Do not create an empty commit.\n";
    prompt += "If unrelated dirty files block a clean scoped commit, set committed=false rather than scooping them in.\n";
    prompt += "Use a HEREDOC commit message in this repo's style, for example:\n";
    prompt += "git commit -m \"$(cat <<'EOF'\n";
    prompt += "refactor(backend): 完成 Service 分层整改波次 " + wave.id + "\n";
    prompt += "\n";
    prompt += "关闭 " + json_encode(closed_ids) + "。批次：" + batch_id + "。\n";
    prompt += "EOF\n";
    prompt += ")\"\n";
    prompt += "After a successful commit, commit_sha must be `git rev-parse HEAD`. If not committed, commit_sha must be the string none.\n";
    prompt
}

let waves = catalog_waves();
let catalog_ids = catalog_item_ids(waves);

let list_waves = false;
let dry_run = false;
let wave_arg = "next";
let max_waves = 1;
let max_clusters = 6;
let include_blocked = false;
let allow_p1 = false;
let do_commit = true;
let custom_ids = [];
let batch_date = "WAVE";
let dirty_note = "Preserve all pre-existing dirty files. Do not revert unrelated work.";
let smoke_merge = false;

if args != () {
    if args.list_waves != () { list_waves = is_true(args.list_waves); }
    if args.dry_run != () { dry_run = is_true(args.dry_run); }
    if args.smoke_merge != () { smoke_merge = is_true(args.smoke_merge); }
    if is_text(args.wave) { wave_arg = args.wave; }
    if args.max_waves != () { max_waves = integer_or(args.max_waves, 1); }
    if args.max_clusters != () { max_clusters = integer_or(args.max_clusters, 6); }
    if args.include_blocked != () { include_blocked = is_true(args.include_blocked); }
    if args.allow_p1_with_open_p0 != () { allow_p1 = is_true(args.allow_p1_with_open_p0); }
    if args.commit != () { do_commit = is_true(args.commit); }
    if is_text_array(args.item_ids) { custom_ids = args.item_ids; }
    if is_text(args.batch_date) { batch_date = args.batch_date; }
    if is_text(args.dirty_note) { dirty_note = args.dirty_note; }
}

if max_waves < 1 { max_waves = 1; }
if max_waves > 12 { max_waves = 12; }
if max_clusters < 1 { max_clusters = 1; }
if max_clusters > 8 { max_clusters = 8; }

if smoke_merge {
    let sample = find_wave(waves, "p0-proc-po-plan");
    let groups = [];
    if sample != () {
        groups = merge_overlapping_clusters(sample.clusters);
    }
    let names = [];
    for group in groups { names.push(group.name); }
    complete(#{
        summary: groups.len().to_string() + " merged groups for p0-proc-po-plan",
        groups: names,
    });
}

if list_waves {
    let listing = [];
    for wave in waves {
        listing.push(#{
            id: wave.id,
            kind: wave.kind,
            title: wave.title,
            item_ids: ids_in_wave(wave),
            cluster_count: wave.clusters.len(),
        });
    }
    let listing_path = write_scratch_file("layering-waves.json", json_encode(listing));
    complete(#{
        summary: listing.len().to_string() + " waves catalogued; P0 waves must finish before P1",
        report_path: listing_path,
        waves: listing,
    });
}

if custom_ids.len() > 0 {
    let filtered_custom = [];
    for item_id in custom_ids {
        if known_item_id(item_id) && array_contains(catalog_ids, item_id) {
            filtered_custom = add_unique(filtered_custom, item_id);
        } else if is_text(item_id) {
            log("Dropped unknown or out-of-catalog item_ids entry: " + item_id);
        }
    }
    custom_ids = filtered_custom;
    if custom_ids.len() == 0 {
        pause("verification", "args.item_ids did not match the hardcoded remaining catalog.");
    }
}

phase("Inventory");
let status_prompt = "";
status_prompt += "Inventory docs/service-layering-remediation-contract.md with grep, not a full-file read. ";
status_prompt += "Extract every stable ID in section 6 tables (CORE/APP/MASTER/SALES/PROC/FUL/FIN/INT + R/E/S numbers). ";
status_prompt += "Return open_ids, blocked_ids, in_progress_ids as they appear in the Status column right now, plus done_count. ";
status_prompt += "Do not infer DONE from code. Quote evidence from matching table rows. Empty arrays are valid only after scanning every domain table.\n";
let status_result = agent(status_prompt, #{
    label: "inventory:contract-status",
    capability_mode: "read-only",
    output_schema: status_schema,
});

let open_ids = [];
let blocked_ids = [];
let in_progress_ids = [];
let done_count = 0;
if status_result != () && status_result.success && status_result.output != () {
    if is_text_array(status_result.output.open_ids) {
        for item_id in status_result.output.open_ids {
            if known_item_id(item_id) { open_ids = add_unique(open_ids, item_id); }
        }
    }
    if is_text_array(status_result.output.blocked_ids) {
        for item_id in status_result.output.blocked_ids {
            if known_item_id(item_id) { blocked_ids = add_unique(blocked_ids, item_id); }
        }
    }
    if is_text_array(status_result.output.in_progress_ids) {
        for item_id in status_result.output.in_progress_ids {
            if known_item_id(item_id) { in_progress_ids = add_unique(in_progress_ids, item_id); }
        }
    }
    done_count = integer_or(status_result.output.done_count, 0);
} else {
    pause("infra", "Contract status inventory failed. Resume to retry.");
}

open_ids = subtract_ids(open_ids, blocked_ids);
if !include_blocked {
    log("Blocked IDs excluded from implementation: " + json_encode(blocked_ids));
}

let work_ids = open_ids;
if include_blocked {
    for item_id in blocked_ids { work_ids = add_unique(work_ids, item_id); }
}
if custom_ids.len() > 0 {
    work_ids = intersect_ids(custom_ids, work_ids);
}

let p0_still_open = any_kind_open(waves, "p0", work_ids);
log("Inventory: open=" + open_ids.len().to_string() + " blocked=" + blocked_ids.len().to_string() + " in_progress=" + in_progress_ids.len().to_string() + " done=" + done_count.to_string());

if dry_run {
    let remaining_waves = [];
    for wave in waves {
        let leftover = intersect_ids(ids_in_wave(wave), work_ids);
        if leftover.len() > 0 {
            remaining_waves.push(#{
                id: wave.id,
                kind: wave.kind,
                title: wave.title,
                remaining_ids: leftover,
            });
        }
    }
    let report = "# Service layering remaining waves\n\n";
    report += "- OPEN (executable): " + open_ids.len().to_string() + "\n";
    report += "- BLOCKED: " + json_encode(blocked_ids) + "\n";
    report += "- P0 still open: " + p0_still_open.to_string() + "\n";
    report += "- Catalog waves with remaining work: " + remaining_waves.len().to_string() + "\n\n";
    report += "```json\n" + json_encode(remaining_waves) + "\n```\n";
    let report_path = write_scratch_file("layering-remaining.md", report);
    complete(#{
        summary: remaining_waves.len().to_string() + " waves still have OPEN items; P0 first",
        report_path: report_path,
        open_count: open_ids.len(),
        blocked_ids: blocked_ids,
        remaining_waves: remaining_waves,
    });
}

if work_ids.len() == 0 {
    complete(#{
        summary: "No executable OPEN items in scope",
        open_count: 0,
        blocked_ids: blocked_ids,
        done_count: done_count,
    });
}

if p0_still_open && (wave_arg == "p1" || wave_arg == "p2") && !allow_p1 {
    pause("verification", "P0 waves still have OPEN items. Pass args.allow_p1_with_open_p0=true only with a recorded approver, or run a p0 wave / next.");
}

if custom_ids.len() > 0 && p0_still_open && !allow_p1 {
    let custom_p0 = false;
    for wave in waves {
        if wave.kind == "p0" {
            if intersect_ids(custom_ids, ids_in_wave(wave)).len() > 0 { custom_p0 = true; }
        }
    }
    if !custom_p0 {
        pause("verification", "Custom item_ids are not a P0 cluster while executable P0 remains. Use a P0 wave or args.allow_p1_with_open_p0=true.");
    }
}

let selected = [];
if custom_ids.len() > 0 {
    selected.push(wave_of("custom", "custom", "Caller-selected catalog IDs", [
        cluster_of("custom", custom_ids, ["backend/services/src/", "backend/entities/src/", "backend/database/src/", "backend/crates/bpm/src/"], true),
    ]));
} else if wave_arg != "next" && wave_arg != "p0" && wave_arg != "p1" && wave_arg != "p2" {
    let found = find_wave(waves, wave_arg);
    if found == () {
        pause("verification", "Unknown args.wave. Pass list_waves=true to see ids, or use next/p0/p1.");
    }
    if found.kind == "blocked" && !include_blocked {
        pause("verification", "Wave blocked-returns-occupancy requires args.include_blocked=true after finance owners sign occupancy policy.");
    }
    if found.kind != "p0" && p0_still_open && !allow_p1 {
        pause("verification", "Refusing non-P0 wave while executable P0 remains. Run wave=next or a p0-* id.");
    }
    if wave_open_count(found, work_ids) == 0 {
        complete(#{
            summary: "Wave " + found.id + " has no remaining OPEN items",
            wave: found.id,
        });
    }
    selected.push(found);
} else {
    let want_kind = "";
    if wave_arg == "p0" || wave_arg == "p1" || wave_arg == "p2" { want_kind = wave_arg; }
    for wave in waves {
        if selected.len() < max_waves {
            let kind_ok = true;
            if want_kind != "" && wave.kind != want_kind { kind_ok = false; }
            if wave.kind == "blocked" && !include_blocked { kind_ok = false; }
            if wave.kind != "p0" && p0_still_open && !allow_p1 { kind_ok = false; }
            if kind_ok && wave_open_count(wave, work_ids) > 0 {
                selected.push(wave);
            }
        }
    }
}

if selected.len() == 0 {
    complete(#{
        summary: "No matching remaining wave for args.wave=" + wave_arg,
        open_count: open_ids.len(),
        blocked_ids: blocked_ids,
    });
}

let selected_names = [];
for wave in selected { selected_names.push(wave.id); }
log("Selected waves: " + json_encode(selected_names));

let wave_reports = [];
let closed_total = [];
let held_total = [];
let commits_total = [];
let quality_passed_all = true;

for wave in selected {
    if remaining_budget() < 20 {
        log("Stopping before " + wave.id + ": remaining agent budget " + remaining_budget().to_string() + " < 20");
        quality_passed_all = false;
        break;
    }

    let batch_id = batch_date + "-" + wave.id;
    let planned_clusters = [];
    for cluster in wave.clusters {
        let leftover = intersect_ids(cluster.item_ids, work_ids);
        if leftover.len() > 0 {
            planned_clusters.push(#{
                name: cluster.name,
                item_ids: leftover,
                prefixes: cluster.prefixes,
                needs_mongo: cluster.needs_mongo,
            });
        }
    }
    if planned_clusters.len() == 0 {
        log("Wave " + wave.id + " had no remaining OPEN clusters");
        continue;
    }

    let merged_all = merge_overlapping_clusters(planned_clusters);
    let impl_groups = [];
    for group in merged_all {
        if impl_groups.len() < max_clusters {
            impl_groups.push(group);
        } else {
            log("Deferred extra merged group in " + wave.id + ": " + group.name);
        }
    }
    let merged_names = [];
    for group in impl_groups { merged_names.push(group.name); }
    log("Implement groups for " + wave.id + ": " + json_encode(merged_names));

    let need_agents = impl_groups.len() * 3 + 6;
    if remaining_budget() < need_agents {
        log("Stopping before implement on " + wave.id + ": remaining budget " + remaining_budget().to_string() + " < " + need_agents.to_string());
        quality_passed_all = false;
        break;
    }

    phase("Implement");
    let implementation_results = [];
    let impl_agent_ids = [];
    for cluster in impl_groups {
        implementation_results.push(fallback_impl(cluster));
        impl_agent_ids.push("");
    }
    let impl_rounds = write_rounds_for(impl_groups);
    log("Implement write rounds for " + wave.id + ": " + impl_rounds.len().to_string());
    for round in impl_rounds {
        let impl_jobs = [];
        let round_indexes = [];
        let gi = 0;
        for cluster in impl_groups {
            let in_round = false;
            for round_group in round {
                if round_group.name == cluster.name { in_round = true; }
            }
            if in_round {
                impl_jobs.push(#{
                    prompt: implement_prompt_for(wave, cluster, batch_id, dirty_note),
                    label: "implement:" + wave.id + ":" + cluster.name,
                    capability_mode: "all",
                    output_schema: implementation_schema,
                });
                round_indexes.push(gi);
            }
            gi += 1;
        }
        if impl_jobs.len() > 0 {
            if remaining_budget() < impl_jobs.len() {
                log("Not enough budget for implement round on " + wave.id + "; fail-closed");
                quality_passed_all = false;
                wave_reports.push(#{ wave: wave.id, status: "implement_budget", groups: merged_names });
                break;
            }
            let impl_raw = parallel(impl_jobs);
            let impl_pos = 0;
            for mapped in round_indexes {
                let impl_result = ();
                if impl_pos < impl_raw.len() { impl_result = impl_raw[impl_pos]; }
                if mapped < impl_groups.len() {
                    implementation_results[mapped] = impl_from_agent(impl_groups[mapped], impl_result);
                    impl_agent_ids[mapped] = agent_id_of(impl_result);
                }
                impl_pos += 1;
            }
        }
    }
    if !quality_passed_all { break; }

    let successful_impls = 0;
    for impl_output in implementation_results {
        if is_true(impl_output.success) { successful_impls += 1; }
    }

    if successful_impls == 0 {
        log("All implementers failed for " + wave.id + "; fail-closed, skipping this and later waves");
        wave_reports.push(#{ wave: wave.id, status: "implement_failed", groups: merged_names });
        quality_passed_all = false;
        break;
    }

    phase("Review");
    let review_jobs = [];
    let review_group_indexes = [];
    let review_index = 0;
    for cluster in impl_groups {
        let impl_ok = false;
        if review_index < implementation_results.len() {
            impl_ok = is_true(implementation_results[review_index].success);
        }
        if impl_ok {
            let impl_payload = json_encode(implementation_results[review_index]);
            review_jobs.push(#{
                prompt: review_prompt_for(wave, cluster, impl_payload),
                label: "review:" + wave.id + ":" + cluster.name,
                capability_mode: "read-only",
                output_schema: review_schema,
            });
            review_group_indexes.push(review_index);
        } else {
            log("Skipping review for failed implementer " + cluster.name);
        }
        review_index += 1;
    }
    let review_results = [];
    if review_jobs.len() > 0 {
        if remaining_budget() < review_jobs.len() {
            log("Not enough budget for review panel on " + wave.id + "; fail-closed");
            quality_passed_all = false;
            wave_reports.push(#{ wave: wave.id, status: "review_budget" });
            break;
        }
        review_results = parallel(review_jobs);
    }

    let reviews_by_group = [];
    for cluster in impl_groups { reviews_by_group.push(()); }
    let review_pos = 0;
    for mapped in review_group_indexes {
        if review_pos < review_results.len() {
            reviews_by_group[mapped] = review_results[review_pos];
        }
        review_pos += 1;
    }

    phase("Repair");
    let repair_jobs = [];
    let repair_indexes = [];
    let repair_index = 0;
    for cluster in impl_groups {
        let impl_ok = false;
        if repair_index < implementation_results.len() {
            impl_ok = is_true(implementation_results[repair_index].success);
        }
        let impl_agent_id = "";
        if repair_index < impl_agent_ids.len() { impl_agent_id = impl_agent_ids[repair_index]; }
        let review = ();
        if repair_index < reviews_by_group.len() { review = reviews_by_group[repair_index]; }
        let needs_repair = false;
        let findings_payload = "review unavailable; treat as failed";
        if impl_ok {
            if review != () && review.success && review.output != () {
                findings_payload = review_findings_payload(review);
                if !is_true(review.output.passed) { needs_repair = true; }
            } else {
                needs_repair = true;
            }
        }
        if needs_repair && impl_ok {
            let resumed = is_text(impl_agent_id);
            let job = #{
                prompt: repair_prompt_for(wave, cluster, findings_payload, dirty_note, resumed),
                label: "repair:" + wave.id + ":" + cluster.name,
                capability_mode: "all",
                output_schema: implementation_schema,
            };
            if resumed { job.resume_from = impl_agent_id; }
            repair_jobs.push(job);
            repair_indexes.push(repair_index);
        }
        repair_index += 1;
    }
    if repair_jobs.len() > 0 {
        if remaining_budget() < repair_jobs.len() {
            log("Not enough budget for review-repair panel on " + wave.id + "; fail-closed");
            quality_passed_all = false;
            wave_reports.push(#{ wave: wave.id, status: "repair_budget", groups: merged_names });
            break;
        }
        let repair_raw = parallel(repair_jobs);
        let repair_pos = 0;
        for mapped in repair_indexes {
            let repaired = ();
            if repair_pos < repair_raw.len() { repaired = repair_raw[repair_pos]; }
            if mapped < impl_groups.len() {
                let repaired_out = impl_from_agent(impl_groups[mapped], repaired);
                if is_true(repaired_out.success) {
                    implementation_results[mapped] = repaired_out;
                }
            }
            repair_pos += 1;
        }
    }

    let mongo_needed = false;
    for cluster in impl_groups {
        if is_true(cluster.needs_mongo) { mongo_needed = true; }
    }

    phase("Quality");
    let quality_result = agent(quality_prompt_for(
        wave,
        batch_id,
        json_encode(implementation_results),
        json_encode(review_results),
        mongo_needed.to_string(),
        false,
        ""
    ), #{
        label: "quality:" + wave.id,
        capability_mode: "all",
        output_schema: quality_schema,
    });
    let quality_agent_id = agent_id_of(quality_result);
    let quality_attempts = 1;
    let gate_repaired = false;

    let quality_usable = quality_result != () && quality_result.success && quality_result.output != ();
    let wave_passed = false;
    let env_blocked = false;
    if quality_usable {
        wave_passed = is_true(quality_result.output.passed);
        env_blocked = is_true(quality_result.output.environment_blocked);
    }

    let failed_paths = [];
    if quality_usable { failed_paths = quality_failed_paths(quality_result.output); }
    let should_gate_repair = false;
    if quality_usable && !wave_passed {
        if env_blocked {
            if failed_paths.len() > 0 { should_gate_repair = true; }
        } else {
            should_gate_repair = true;
        }
    }

    if should_gate_repair {
        phase("Gate-repair");
        let need_flags = [];
        for cluster in impl_groups { need_flags.push(false); }
        let unmatched_paths = [];
        for path in failed_paths {
            let owner_count = 0;
            let last_owner = -1;
            let gi = 0;
            for cluster in impl_groups {
                let impl_ok = false;
                let files = [];
                if gi < implementation_results.len() {
                    impl_ok = is_true(implementation_results[gi].success);
                    if is_text_array(implementation_results[gi].files_changed) {
                        files = implementation_results[gi].files_changed;
                    }
                }
                if impl_ok && group_owns_path(cluster.prefixes, files, path) {
                    owner_count += 1;
                    last_owner = gi;
                }
                gi += 1;
            }
            if owner_count == 1 {
                need_flags[last_owner] = true;
            } else {
                unmatched_paths = add_unique(unmatched_paths, path);
            }
        }
        if failed_paths.len() == 0 {
            unmatched_paths = add_unique(unmatched_paths, "(unattributed quality failure)");
        }

        let findings_blob = json_encode(quality_issue_texts(quality_result.output));
        if quality_usable {
            findings_blob = json_encode(#{
                failed_paths: failed_paths,
                remaining_issues: quality_issue_texts(quality_result.output),
                summary: quality_result.output.summary,
            });
        }

        let gate_jobs = [];
        let gate_group_indexes = [];
        if unmatched_paths.len() > 0 {
            let union_prefixes = [];
            for cluster in impl_groups {
                if is_text_array(cluster.prefixes) {
                    for prefix in cluster.prefixes { union_prefixes = add_unique(union_prefixes, prefix); }
                }
            }
            let unmatched_blob = json_encode(#{
                unmatched_paths: unmatched_paths,
                remaining_issues: quality_issue_texts(quality_result.output),
                failed_paths: failed_paths,
            });
            gate_jobs.push(#{
                prompt: integrator_prompt_for(wave, unmatched_blob, dirty_note, union_prefixes),
                label: "repair-gate:" + wave.id + ":integrator",
                capability_mode: "all",
                output_schema: implementation_schema,
            });
        } else {
            let gi2 = 0;
            for cluster in impl_groups {
                if gi2 < need_flags.len() {
                    if need_flags[gi2] {
                        let impl_agent_id = "";
                        if gi2 < impl_agent_ids.len() { impl_agent_id = impl_agent_ids[gi2]; }
                        let resumed = is_text(impl_agent_id);
                        let job = #{
                            prompt: gate_repair_prompt_for(wave, cluster, findings_blob, dirty_note, resumed),
                            label: "repair-gate:" + wave.id + ":" + cluster.name,
                            capability_mode: "all",
                            output_schema: implementation_schema,
                        };
                        if resumed { job.resume_from = impl_agent_id; }
                        gate_jobs.push(job);
                        gate_group_indexes.push(gi2);
                    }
                }
                gi2 += 1;
            }
        }

        if gate_jobs.len() > 0 {
            if remaining_budget() < gate_jobs.len() + 1 {
                log("Not enough budget for gate-repair on " + wave.id + "; fail-closed");
                quality_passed_all = false;
                wave_reports.push(#{ wave: wave.id, status: "gate_repair_budget", groups: merged_names });
                break;
            }
            let gate_raw = parallel(gate_jobs);
            gate_repaired = true;
            let gpos = 0;
            for mapped in gate_group_indexes {
                let repaired = ();
                if gpos < gate_raw.len() { repaired = gate_raw[gpos]; }
                if mapped < impl_groups.len() {
                    let repaired_out = impl_from_agent(impl_groups[mapped], repaired);
                    if is_true(repaired_out.success) {
                        implementation_results[mapped] = repaired_out;
                    }
                }
                gpos += 1;
            }

            phase("Quality");
            let retry_opts = #{
                label: "quality-retry:" + wave.id,
                capability_mode: "all",
                output_schema: quality_schema,
            };
            if is_text(quality_agent_id) { retry_opts.resume_from = quality_agent_id; }
            quality_result = agent(quality_prompt_for(
                wave,
                batch_id,
                json_encode(implementation_results),
                json_encode(review_results),
                mongo_needed.to_string(),
                true,
                findings_blob
            ), retry_opts);
            quality_attempts = 2;
            quality_usable = quality_result != () && quality_result.success && quality_result.output != ();
            wave_passed = false;
            if quality_usable { wave_passed = is_true(quality_result.output.passed); }
        } else {
            log("Quality failed for " + wave.id + " with no attributable repair jobs");
        }
    }

    let wave_closed = [];
    let wave_held = [];
    if quality_usable {
        if is_text_array(quality_result.output.closed_ids) {
            wave_closed = intersect_ids(quality_result.output.closed_ids, ids_in_wave(wave));
        }
        if is_text_array(quality_result.output.held_ids) {
            wave_held = quality_result.output.held_ids;
        }
    }
    if !wave_passed { quality_passed_all = false; }
    for item_id in wave_closed { closed_total = add_unique(closed_total, item_id); }
    for item_id in wave_held { held_total = add_unique(held_total, item_id); }
    work_ids = subtract_ids(work_ids, wave_closed);
    p0_still_open = any_kind_open(waves, "p0", work_ids);

    let commit_output = #{
        committed: false,
        commit_sha: "none",
        files_staged: [],
        skipped_dirty: [],
        summary: "commit skipped",
    };
    if do_commit && wave_passed && wave_closed.len() > 0 {
        phase("Commit");
        let quality_payload = "quality unavailable";
        if quality_result != () { quality_payload = json_encode(quality_result); }
        let commit_result = agent(commit_prompt_for(wave, batch_id, wave_closed, quality_payload), #{
            label: "commit:" + wave.id,
            capability_mode: "all",
            output_schema: commit_schema,
        });
        if commit_result != () && commit_result.success && commit_result.output != () {
            commit_output = #{
                committed: is_true(commit_result.output.committed),
                commit_sha: commit_result.output.commit_sha,
                files_staged: commit_result.output.files_staged,
                skipped_dirty: commit_result.output.skipped_dirty,
                summary: commit_result.output.summary,
            };
            if !is_text(commit_output.commit_sha) { commit_output.commit_sha = "none"; }
            if is_true(commit_output.committed) && commit_output.commit_sha != "none" {
                commits_total.push(#{ wave: wave.id, commit_sha: commit_output.commit_sha });
            }
        } else {
            commit_output.summary = "commit agent failed";
            quality_passed_all = false;
        }
    } else if !do_commit {
        commit_output.summary = "args.commit=false";
    } else if !wave_passed {
        commit_output.summary = "quality did not pass; left uncommitted";
    } else {
        commit_output.summary = "no closed_ids; left uncommitted";
    }
    if do_commit && wave_passed && wave_closed.len() > 0 && !is_true(commit_output.committed) {
        quality_passed_all = false;
    }

    wave_reports.push(#{
        wave: wave.id,
        passed: wave_passed,
        closed_ids: wave_closed,
        held_ids: wave_held,
        committed: commit_output.committed,
        commit_sha: commit_output.commit_sha,
        commit_summary: commit_output.summary,
        groups: merged_names,
        quality_attempts: quality_attempts,
        gate_repaired: gate_repaired,
        quality: quality_result,
    });

    if !quality_passed_all {
        log("Stopping later waves after " + wave.id);
        break;
    }
}

phase("Report");
let report = "# Service layering remediation run\n\n";
report += "- Wave arg: " + wave_arg + "\n";
report += "- Closed this run: " + json_encode(closed_total) + "\n";
report += "- Held: " + json_encode(held_total) + "\n";
report += "- Quality passed all selected waves: " + quality_passed_all.to_string() + "\n";
report += "- Per-wave commit: " + do_commit.to_string() + " (one commit after Quality pass with closed_ids; no commit on failure)\n";
report += "- Remaining executable OPEN after run (inventory minus closed): " + work_ids.len().to_string() + "\n";
report += "- BLOCKED still excluded: " + json_encode(blocked_ids) + "\n\n";
report += "Next: re-run with args.wave=next (or the next p0-* id) until P0 waves are empty, then P1 waves. ";
report += "Implementers do not run cargo; Quality is the only compile/test step (one retry after code-only gate-repair). ";
report += "This run stops after implement/quality/commit failure or when remaining agent budget is too low; do not pass max_waves=12 unless you intend to chain only while gates pass. ";
report += "Do not implement blocked-returns-occupancy until finance owners sign occupancy policy.\n\n";
report += "## Wave results\n\n```json\n" + json_encode(wave_reports) + "\n```\n";
let report_path = write_scratch_file("service-layering-remediation-report.md", report);

complete(#{
    summary: closed_total.len().to_string() + " IDs closed, " + held_total.len().to_string() + " held, commits=" + commits_total.len().to_string() + ", quality_passed_all=" + quality_passed_all.to_string(),
    report_path: report_path,
    closed_ids: closed_total,
    held_ids: held_total,
    commits: commits_total,
    quality_passed_all: quality_passed_all,
    remaining_open_estimate: work_ids.len(),
});
