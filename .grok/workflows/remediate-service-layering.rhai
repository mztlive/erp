let meta = #{
    name: "remediate-service-layering",
    description: "按合同关闭剩余 OPEN 责任簇：前缀不相交并行实现/评审/修复/核验，统一纯单元测试门禁，失败按不相交前缀并行修复并重跑直到可提交",
    when_to_use: "当需要执行后端 Service 分层剩余整改合同时。发现新问题请用 sink-service-layering；本 workflow 只关闭合同第 6 节已编号且仍为 OPEN 的项，不实施 BLOCKED 项。Verify 未通过的 ID 不得关闭。Quality 只跑 cargo test --workspace --lib，禁止任何集成测试；门禁失败按不相交前缀并行派出修复 agent，重跑直到单元测试全绿且提交成功。",
    phases: [
        #{ title: "Inventory", detail: "读取合同 OPEN/BLOCKED 状态并与波次目录求交" },
        #{ title: "Plan", detail: "并行确认每个簇仍 OPEN 且代码未改完" },
        #{ title: "Implement", detail: "不相交组合分片并行实现；相交组合顺序分片。隔离 target 跑定向单元测试" },
        #{ title: "Review", detail: "每个实现成功的组合并行对抗评审" },
        #{ title: "Repair", detail: "按不相交分片并行 resume 修复本簇评审 blocker" },
        #{ title: "Verify", detail: "只读对抗核验关闭证据；失败不得进入关闭集" },
        #{ title: "Quality", detail: "纯单元测试统一门禁，不跑集成测试；unit_tests_passed 为硬条件" },
        #{ title: "QualityRepair", detail: "门禁失败按不相交前缀并行修复，再重跑 Quality，直到全绿或预算耗尽" },
        #{ title: "Commit", detail: "Quality 通过且有关闭项时提交；失败则重试直到提交成功" },
        #{ title: "Report", detail: "关闭证据、分片并行度、门禁闭环和提交结果" },
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

let cluster_plan_schema = #{
    "type": "object",
    "required": ["name", "still_open", "code_still_wrong", "needs_mongo", "item_ids", "skip_reason"],
    "properties": #{
        "name": #{ "type": "string" },
        "still_open": #{ "type": "boolean" },
        "code_still_wrong": #{ "type": "boolean" },
        "needs_mongo": #{ "type": "boolean" },
        "item_ids": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
        "skip_reason": #{ "type": "string" },
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

let verify_schema = #{
    "type": "object",
    "required": ["cluster", "passed", "completed_ids", "held_ids", "evidence", "issues", "summary"],
    "properties": #{
        "cluster": #{ "type": "string" },
        "passed": #{ "type": "boolean" },
        "completed_ids": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
        "held_ids": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
        "issues": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let quality_repair_schema = #{
    "type": "object",
    "required": ["files_changed", "summary", "remaining_issues"],
    "properties": #{
        "files_changed": #{ "type": "array", "maxItems": 120, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
        "remaining_issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
    },
};

let quality_schema = #{
    "type": "object",
    "required": [
        "passed", "unit_tests_passed", "closed_ids", "held_ids", "commands",
        "files_changed", "remaining_issues", "failure_groups", "summary"
    ],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "unit_tests_passed": #{ "type": "boolean" },
        "closed_ids": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "held_ids": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 120, "items": #{ "type": "string" } },
        "remaining_issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "failure_groups": #{
            "type": "array",
            "maxItems": 12,
            "items": #{
                "type": "object",
                "required": ["name", "prefixes", "issues", "commands"],
                "properties": #{
                    "name": #{ "type": "string" },
                    "prefixes": #{ "type": "array", "maxItems": 8, "items": #{ "type": "string" } },
                    "issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
                    "commands": #{ "type": "array", "maxItems": 8, "items": #{ "type": "string" } },
                },
            },
        },
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

fn take_prefix(items, n) {
    let out = [];
    let i = 0;
    if is_text_array(items) || type_of(items) == "array" {
        for item in items {
            if i < n { out.push(item); }
            i += 1;
        }
    }
    out
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
    if group.member_names.len() < 3 {
        let ids = [];
        for item_id in group.item_ids { ids = add_unique(ids, item_id); }
        for item_id in cluster.item_ids { ids = add_unique(ids, item_id); }
        if ids.len() <= 12 { ok = true; }
    }
    ok
}

fn wave_ids_from(cluster) {
    let ids = [];
    if is_text_array(cluster.wave_ids) {
        for item in cluster.wave_ids { ids = add_unique(ids, item); }
    }
    if is_text(cluster.wave_id) { ids = add_unique(ids, cluster.wave_id); }
    ids
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
    let wave_ids = wave_ids_from(group);
    for item in wave_ids_from(cluster) { wave_ids = add_unique(wave_ids, item); }
    let needs_mongo = is_true(group.needs_mongo);
    if is_true(cluster.needs_mongo) { needs_mongo = true; }
    #{
        name: join_text(names, "+"),
        item_ids: ids,
        prefixes: prefixes,
        needs_mongo: needs_mongo,
        member_names: names,
        wave_ids: wave_ids,
    }
}

fn cluster_as_group(cluster) {
    let names = [];
    names = add_unique(names, cluster.name);
    #{
        name: cluster.name,
        item_ids: cluster.item_ids,
        prefixes: cluster.prefixes,
        needs_mongo: cluster.needs_mongo,
        member_names: names,
        wave_ids: wave_ids_from(cluster),
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

fn index_slice_can_take(slice, groups, gi, max_parallel) {
    let ok = false;
    if slice.len() < max_parallel {
        if gi < groups.len() {
            let clash = false;
            for idx in slice {
                if idx < groups.len() {
                    if prefixes_conflict(groups[idx].prefixes, groups[gi].prefixes) {
                        clash = true;
                    }
                }
            }
            if !clash { ok = true; }
        }
    }
    ok
}

fn pack_index_slices(groups, max_parallel) {
    let slices = [];
    let gi = 0;
    for group in groups {
        let placed = false;
        let next_slices = [];
        for slice in slices {
            if !placed && index_slice_can_take(slice, groups, gi, max_parallel) {
                let grown = [];
                for idx in slice { grown.push(idx); }
                grown.push(gi);
                next_slices.push(grown);
                placed = true;
            } else {
                next_slices.push(slice);
            }
        }
        if !placed {
            let first = [];
            first.push(gi);
            next_slices.push(first);
        }
        slices = next_slices;
        gi += 1;
    }
    slices
}

fn names_for_slice(groups, slice) {
    let names = [];
    for idx in slice {
        if idx < groups.len() { names.push(groups[idx].name); }
    }
    names
}

fn slice_preview(groups, slices) {
    let out = [];
    let si = 0;
    for slice in slices {
        out.push(#{
            slice: si,
            size: slice.len(),
            groups: names_for_slice(groups, slice),
        });
        si += 1;
    }
    out
}

fn launch_parallel(jobs, reserve) {
    let results = [];
    if jobs.len() == 0 { return results; }
    if remaining_budget() < jobs.len() + reserve {
        log("skip parallel panel; jobs=" + jobs.len().to_string() + " reserve=" + reserve.to_string() + " remaining=" + remaining_budget().to_string());
        return results;
    }
    parallel(jobs)
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

fn review_passed(review) {
    let ok = false;
    if review != () && review.success && review.output != () {
        if is_true(review.output.passed) {
            let has_blocker = false;
            if type_of(review.output.issues) == "array" {
                for issue in review.output.issues {
                    if issue != () {
                        if issue.severity == "blocker" { has_blocker = true; }
                    }
                }
            }
            if !has_blocker { ok = true; }
        }
    }
    ok
}

fn commands_ran_integration(commands) {
    let found = false;
    if is_text_array(commands) {
        for command in commands {
            if is_text(command) {
                if command.contains("--include-ignored") { found = true; }
                if command.contains("ERP_TEST_MONGO") { found = true; }
                if command.contains("dev-mongo") { found = true; }
                if command.contains("docker compose") { found = true; }
                if command.contains("--test ") { found = true; }
                if command.contains("cargo test") {
                    if !command.contains("--lib") { found = true; }
                }
            }
        }
    }
    found
}

fn quality_passed(result) {
    let ok = false;
    if result != () && result.success && result.output != () {
        if is_true(result.output.passed) {
            if is_true(result.output.unit_tests_passed) {
                if !commands_ran_integration(result.output.commands) {
                    ok = true;
                }
            }
        }
    }
    ok
}

fn quality_issues_text(result) {
    let text = "quality-unavailable";
    if result != () && result.success && result.output != () {
        if type_of(result.output.failure_groups) == "array" && result.output.failure_groups.len() > 0 {
            text = json_encode(result.output.failure_groups);
        } else if is_text_array(result.output.remaining_issues) {
            text = json_encode(result.output.remaining_issues);
        } else if is_text(result.output.summary) {
            text = result.output.summary;
        }
    }
    text
}

fn quality_fingerprint_of(result) {
    let text = quality_issues_text(result);
    let fp = fingerprint(text);
    if !is_text(fp) { fp = text; }
    fp
}

fn sanitize_repair_prefixes(prefixes) {
    let out = [];
    if is_text_array(prefixes) {
        for prefix in prefixes {
            if is_text(prefix) {
                let allowed = false;
                if prefix.starts_with("backend/") { allowed = true; }
                if prefix.starts_with("docs/") { allowed = true; }
                if prefix.contains("/tests/") { allowed = false; }
                if prefix.contains("tests/") { allowed = false; }
                if allowed { out = add_unique(out, prefix); }
            }
        }
    }
    if out.len() == 0 { out.push("backend/"); }
    out
}

fn prefixes_overlap(left, right) {
    let found = false;
    if is_text_array(left) {
        if is_text_array(right) {
            for pa in left {
                if is_text(pa) && pa != "" {
                    for pb in right {
                        if is_text(pb) && pb != "" {
                            if pa == pb { found = true; }
                            if pa.starts_with(pb) { found = true; }
                            if pb.starts_with(pa) { found = true; }
                        }
                    }
                }
            }
        }
    }
    found
}

fn repair_slice_can_take(slice, groups, gi, max_jobs) {
    let ok = false;
    if slice.len() < max_jobs {
        if gi < groups.len() {
            let clash = false;
            for idx in slice {
                if idx < groups.len() {
                    if prefixes_overlap(groups[idx].prefixes, groups[gi].prefixes) {
                        clash = true;
                    }
                }
            }
            if !clash { ok = true; }
        }
    }
    ok
}

fn pack_repair_slices(groups, max_jobs) {
    let slices = [];
    let gi = 0;
    for group in groups {
        let placed = false;
        let next_slices = [];
        for slice in slices {
            if !placed && repair_slice_can_take(slice, groups, gi, max_jobs) {
                let grown = [];
                for idx in slice { grown.push(idx); }
                grown.push(gi);
                next_slices.push(grown);
                placed = true;
            } else {
                next_slices.push(slice);
            }
        }
        if !placed {
            let first = [];
            first.push(gi);
            next_slices.push(first);
        }
        slices = next_slices;
        gi += 1;
    }
    slices
}

fn fallback_failure_group(result) {
    let issues = [];
    if result != () && result.success && result.output != () {
        if is_text_array(result.output.remaining_issues) {
            for issue in result.output.remaining_issues {
                if is_text(issue) { issues = add_unique(issues, issue); }
            }
        }
        if is_text(result.output.summary) { issues = add_unique(issues, result.output.summary); }
    }
    if issues.len() == 0 { issues.push("unspecified unit-gate failure; treat as workspace breakage"); }
    #{
        name: "workspace",
        prefixes: ["backend/"],
        issues: issues,
        commands: ["cargo test --workspace --lib --all-features --locked"],
    }
}

fn failure_groups_from_quality(result) {
    let groups = [];
    if result != () && result.success && result.output != () {
        if type_of(result.output.failure_groups) == "array" {
            let gi = 0;
            for raw in result.output.failure_groups {
                if raw != () {
                    let name = "group-" + gi.to_string();
                    if is_text(raw.name) { name = raw.name; }
                    let prefixes = sanitize_repair_prefixes(raw.prefixes);
                    let issues = [];
                    if is_text_array(raw.issues) {
                        for issue in raw.issues {
                            if is_text(issue) { issues = add_unique(issues, issue); }
                        }
                    }
                    let commands = [];
                    if is_text_array(raw.commands) {
                        for command in raw.commands {
                            if is_text(command) { commands = add_unique(commands, command); }
                        }
                    }
                    if issues.len() > 0 {
                        groups.push(#{
                            name: name,
                            prefixes: prefixes,
                            issues: issues,
                            commands: commands,
                        });
                    }
                }
                gi += 1;
            }
        }
    }
    if groups.len() == 0 { groups.push(fallback_failure_group(result)); }
    groups
}

fn run_agent_job(job) {
    let opts = #{
        label: job.label,
        capability_mode: job.capability_mode,
        output_schema: job.output_schema,
    };
    if job.resume_from != () {
        if is_text(job.resume_from) { opts.resume_from = job.resume_from; }
    }
    agent(job.prompt, opts)
}

fn launch_jobs_or_seq(jobs, reserve) {
    let results = launch_parallel(jobs, reserve);
    if results.len() == 0 && jobs.len() > 0 {
        log("parallel panel skipped; falling back to sequential agents, jobs=" + jobs.len().to_string());
        for job in jobs {
            if remaining_budget() > reserve {
                results.push(run_agent_job(job));
            } else {
                log("sequential fallback stopped; remaining=" + remaining_budget().to_string());
                results.push(());
            }
        }
    }
    results
}

fn dispatch_quality_repairs(batch_id, quality_result, attempt, stall, preserve_note, max_jobs) {
    let groups = failure_groups_from_quality(quality_result);
    let room = remaining_budget();
    if room > 2 {
        room = room - 2;
    } else {
        room = 1;
    }
    let cap = max_jobs;
    if cap > room { cap = room; }
    if cap < 1 { cap = 1; }
    let slices = pack_repair_slices(groups, cap);
    log("QualityRepair groups=" + groups.len().to_string() + " slices=" + slices.len().to_string() + " cap=" + cap.to_string());
    let qfail = quality_issues_text(quality_result);
    let slice_no = 0;
    for slice in slices {
        let jobs = [];
        let names = [];
        for idx in slice {
            if idx < groups.len() {
                let group = groups[idx];
                names.push(group.name);
                let target_dir = "/tmp/erp-layering-qg" + idx.to_string() + "-r" + attempt.to_string();
                jobs.push(#{
                    prompt: quality_repair_prompt_for_group(batch_id, group, qfail, attempt, stall, target_dir, preserve_note),
                    label: "quality-repair:" + group.name + ":r" + attempt.to_string(),
                    capability_mode: "all",
                    output_schema: quality_repair_schema,
                });
            }
        }
        log("QualityRepair slice " + slice_no.to_string() + ": " + json_encode(names));
        let raw = launch_jobs_or_seq(jobs, 2);
        let ji = 0;
        for slot in raw {
            let name = "slot";
            if ji < names.len() { name = names[ji]; }
            if slot != () && slot.success && slot.output != () {
                log("QualityRepair " + name + ": " + slot.output.summary);
            } else {
                log("QualityRepair " + name + " returned unusable output");
            }
            ji += 1;
        }
        slice_no += 1;
    }
}

fn empty_impl(cluster, summary) {
    #{
        cluster: cluster.name,
        success: false,
        completed_ids: [],
        skipped_ids: cluster.item_ids,
        needs_mongo: cluster.needs_mongo,
        mongo_ran: false,
        summary: summary,
        blockers: [summary],
        files_changed: [],
        checks_run: [],
        agent_id: "",
    }
}

fn impl_from_agent(cluster, result) {
    let out = empty_impl(cluster, "implementer returned unusable output");
    if result != () && result.success && result.output != () {
        let completed = [];
        if is_text_array(result.output.completed_ids) {
            completed = intersect_ids(result.output.completed_ids, cluster.item_ids);
        }
        out = #{
            cluster: cluster.name,
            success: true,
            completed_ids: completed,
            skipped_ids: result.output.skipped_ids,
            needs_mongo: cluster.needs_mongo,
            mongo_ran: is_true(result.output.mongo_ran),
            summary: result.output.summary,
            blockers: result.output.blockers,
            files_changed: result.output.files_changed,
            checks_run: result.output.checks_run,
            agent_id: agent_id_of(result),
        };
    } else {
        out.agent_id = agent_id_of(result);
    }
    out
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

fn ids_in_groups(groups) {
    let ids = [];
    for group in groups {
        for item_id in group.item_ids { ids = add_unique(ids, item_id); }
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

fn any_kind_open(waves, kind, open_ids) {
    let found = false;
    for wave in waves {
        if wave.kind == kind {
            if intersect_ids(ids_in_wave(wave), open_ids).len() > 0 { found = true; }
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
    text += "Follow section 7.2 per cluster: failing or characterizing unit tests first; domain; repository; service switch + delete old path.\n";
    text += "Unit-only gate: run only cargo test --workspace --lib. Never add, modify, or run any integration test: no tests/ directory, no --test target, no --include-ignored, no real MongoDB or external service.\n";
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

fn plan_prompt_for(cluster) {
    let prompt = "";
    prompt += "Independently verify this service-layering cluster against the live contract and code.\n";
    prompt += "Cluster: " + cluster.name + ". Item IDs: " + json_encode(cluster.item_ids) + ".\n";
    prompt += "Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Grep only these ID rows in docs/service-layering-remediation-contract.md; do not read the whole file. Then grep/read_file the cited service/entity/repository files.\n";
    prompt += "still_open=true only if the contract Status is OPEN or IN_PROGRESS. code_still_wrong=true only if the cited problem still exists.\n";
    prompt += "If the row is already DONE, BLOCKED, or the code already matches the forced adjustment, set skip_reason and still_open/code_still_wrong accordingly.\n";
    prompt += "needs_mongo=true when the close checks require aggregation, index explain, CAS, uniqueness, or real transactions.\n";
    prompt += "Keep item_ids inside the planned list. Empty skip_reason means the cluster should be implemented now.\n";
    prompt += "Return name exactly as " + cluster.name + ".\n";
    prompt
}

fn implement_prompt_for(cluster, batch_id, dirty_note, target_dir) {
    let prompt = "";
    prompt += "Implement only this verified service-layering group in the current Rust workspace.\n";
    prompt += "Waves: " + json_encode(cluster.wave_ids) + ".\n";
    prompt += "Group: " + cluster.name + ". Member clusters: " + json_encode(cluster.member_names) + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ".\n";
    prompt += "Allowed prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Batch id to record: " + batch_id + ". Owner: Grok.\n";
    prompt += dirty_note + "\n";
    prompt += layering_rules_text();
    prompt += "PARALLEL SAFETY: other agents are editing disjoint prefixes at the same time. ";
    prompt += "Modify only files under the allowed prefixes plus the matching domain-owned backend/entities, backend/database repository/indexes, or backend/crates/bpm modules those rows name. ";
    prompt += "Do not fmt, touch, or lock files outside this group. Do not edit Cargo.lock unless a new test crate is required (it is not).\n";
    prompt += "Grep only the assigned ID rows in docs/service-layering-remediation-contract.md. Never read that whole file. Then grep/read_file the live source those rows cite. Do not trust this prompt's memory of line numbers.\n";
    prompt += "Inspect git status first. Implement every assigned ID in this group. Shared modules in the group are intentional.\n";
    prompt += "FOCUSED TESTS: after edits, run cargo fmt on touched files, then focused unit tests only: cargo test -p entities and/or -p database and/or -p services --lib with a narrow filter for this group's symbols. ";
    prompt += "You MUST set CARGO_TARGET_DIR=" + target_dir + " for every cargo check/test so you do not race the workspace target. ";
    prompt += "When finished, rm -rf " + target_dir + ". Never export CARGO_TARGET_DIR into the repo, shell rc, or a later Quality agent.\n";
    prompt += "Do not run cargo test --workspace, clippy --workspace, or the three ./scripts/check-*.sh scripts; Quality owns those.\n";
    prompt += "Never add, modify, or run any integration test: no tests/ directory, no --test target, no --include-ignored, no real MongoDB. Leave mongo_ran=false. Write the required unit tests (--lib) if the row needs them.\n";
    prompt += "Return completed_ids only for items whose old rule source is deleted and whose required unit tests exist and the focused cargo test passed. Otherwise put them in skipped_ids with blockers.\n";
    prompt
}

fn review_prompt_for(cluster, impl_payload) {
    let prompt = "";
    prompt += "Adversarially review the current git diff for service-layering group " + cluster.name + ".\n";
    prompt += "Waves: " + json_encode(cluster.wave_ids) + ". Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Implementer payload is a hypothesis only: " + impl_payload + "\n";
    prompt += "Grep only the listed ID rows in docs/service-layering-remediation-contract.md; do not read the whole contract. Inspect the live diff with git diff, grep, and read_file. Do not approve from names.\n";
    prompt += "Fail closed: passed=true only with no concrete blocker.\n";
    prompt += "Must catch: leftover Service helpers; repository business rules or transactions; entity I/O/secrets/clock/id-generator; BPM taking ERP types; ";
    prompt += "missing tests for the row's applicable dimensions; missing method docs; edits to frozen files; public API/schema changes not required by the row; ";
    prompt += "behavior drift in soft-delete, current pointer, money, sort, or first-error; marking DONE in the contract; edits outside allowed prefixes.\n";
    prompt += "Severity blocker|major|minor. Every issue needs file, evidence, and a specific fix. Empty issues is valid only after reading the diff.\n";
    prompt
}

fn repair_prompt_for(cluster, findings_payload, dirty_note, resumed, target_dir) {
    let prompt = "";
    prompt += "Repair only the review blockers for group " + cluster.name + ".\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Review findings (fix these, do not expand scope): " + findings_payload + "\n";
    prompt += dirty_note + "\n";
    if resumed {
        prompt += "This session already implemented this group. Do not re-read the whole contract or rediscover the crate. ";
        prompt += "Use git diff plus the findings list. Do not rerun cargo tests that already passed unless a finding requires it.\n";
    } else {
        prompt += layering_rules_text();
        prompt += "Grep only the assigned ID rows; never read the whole contract file.\n";
    }
    prompt += "PARALLEL SAFETY: only touch this group's prefixes. Do not expand scope.\n";
    prompt += "If you must re-run focused tests, set CARGO_TARGET_DIR=" + target_dir + " and rm -rf it afterwards. Do not run workspace gates.\n";
    prompt += "Do not edit docs/service-layering-remediation-contract.md. Do not git add or git commit. Never run integration tests, MongoDB, or #[ignore] tests.\n";
    prompt
}

fn verify_prompt_for(cluster, impl_payload, review_payload) {
    let prompt = "";
    prompt += "Independently verify whether service-layering group " + cluster.name + " may be closed.\n";
    prompt += "Item IDs: " + json_encode(cluster.item_ids) + ". Prefixes: " + json_encode(cluster.prefixes) + ".\n";
    prompt += "Implementer payload is untrusted: " + impl_payload + "\n";
    prompt += "Review payload is untrusted: " + review_payload + "\n";
    prompt += "This phase is read-only. Use grep, read_file, and git diff. Do not edit files, do not run cargo, do not run Mongo.\n";
    prompt += "Grep only the listed ID rows in docs/service-layering-remediation-contract.md and inspect the live diff plus tests.\n";
    prompt += "passed=true only if EVERY assigned ID that you list in completed_ids has: ";
    prompt += "the forced adjustment present; the old Service helper/rule source deleted (not a forwarder); ";
    prompt += "required unit tests for applicable close-check dimensions; no frozen-file edits; no public API change unless the row requires it; ";
    prompt += "diff confined to allowed prefixes plus matching domain entity/repository/bpm modules.\n";
    prompt += "Fail closed: missing evidence, failed review blockers still in the diff, or unreadable output means passed=false.\n";
    prompt += "completed_ids must be a subset of the assigned IDs. Put the rest in held_ids. evidence must quote files you actually read.\n";
    prompt += "Empty issues is valid only after inspecting the diff. Do not mark the contract DONE.\n";
    prompt
}

fn quality_prompt_for(batch_id, allowed_ids, cluster_payloads, review_payloads, verify_payloads, retest) {
    let prompt = "";
    prompt += "Run the unit-only quality gate for this service-layering batch. Never run integration tests.\n";
    prompt += "Batch id: " + batch_id + ". Owner: Grok.\n";
    prompt += "IDs allowed to close (Verify passed): " + json_encode(allowed_ids) + ".\n";
    prompt += "Inspect git status and diff first. Never git reset/checkout/restore/clean. Preserve unrelated dirty files.\n";
    if retest {
        prompt += "This is a retest after QualityRepair. Re-run the full unit-only gate. Do not expand scope. ";
        prompt += "Close the same allowed IDs if and only if the entire gate is green, including cargo test --workspace --lib with 0 failed.\n";
    } else {
        prompt += "Implementer payloads: " + cluster_payloads + "\n";
        prompt += "Review payloads: " + review_payloads + "\n";
        prompt += "Verify payloads: " + verify_payloads + "\n";
    }
    prompt += "You MAY run `cargo fmt --all` once (write) to absorb format-only noise, then the check sequence. ";
    prompt += "You MAY fix cheap batch-caused fmt/clippy/check failures in place. ";
    prompt += "If `cargo test --workspace --lib` fails, do a short pass on obvious breakage from this batch, then return passed=false with sharded failure_groups so parallel repairers can finish. ";
    prompt += "Do not spend the whole session trying to make a red --lib suite green by yourself.\n";
    prompt += "Keep repository/entity/service/BPM boundaries from contract section 2. ";
    prompt += "Every added or modified method must keep its multi-line docs. Entity rules need direct unit tests. Old Service helpers must stay deleted.\n";
    prompt += "Run from backend, in this exact order:\n";
    prompt += "cargo fmt --all -- --check\n";
    prompt += "cargo check --workspace --all-features --locked\n";
    prompt += "cargo clippy --workspace --all-features --locked -- -D warnings\n";
    prompt += "cargo test --workspace --lib --all-features --locked\n";
    prompt += "./scripts/check-bpm-boundaries.sh\n";
    prompt += "./scripts/check-service-boundaries.sh\n";
    prompt += "./scripts/check-permissions-drift.sh\n";
    prompt += "git diff --check\n";
    prompt += "Do not pass clippy --all-targets: that compiles crate tests/ integration tests. Clippy default lib+bins only.\n";
    prompt += "All cargo commands must use the workspace default target directory only. ";
    prompt += "Do not set CARGO_TARGET_DIR. Do not use /tmp/erp-*-target, /tmp/cargo-*, or any other temporary or dedicated target dir.\n";
    prompt += "UNIT-ONLY HARD GATE: the only allowed test command is cargo test --workspace --lib --all-features --locked. ";
    prompt += "unit_tests_passed=true only if that command exited 0 with 0 failed tests (ignored tests do not count as failures). ";
    prompt += "passed=true is illegal unless unit_tests_passed=true AND the exact --lib command appears in commands. ";
    prompt += "Forbidden: tests/ directory targets, cargo test without --lib, any --test <name> target, --include-ignored, real MongoDB, ";
    prompt += "ERP_TEST_MONGO_URI, dev-mongo.sh, docker compose test profile, or external services. ";
    prompt += "Do not add, modify, or run integration tests. R/E close evidence uses --lib only; Mongo/integration dimensions are N/A.\n";
    prompt += "Put every command you actually ran into commands. The orchestrator rejects passed=true if commands include integration tests.\n";
    prompt += "If the gate is still red, set passed=false, unit_tests_passed from the --lib result, keep closed_ids empty, ";
    prompt += "list concrete errors in remaining_issues, and shard remaining work into failure_groups with disjoint prefixes ";
    prompt += "(typical shards: backend/entities/src, backend/database/src, backend/services/src, backend/crates/bpm/src, backend/scripts, docs). ";
    prompt += "Workspace-wide failures that are not file-local go in one group named workspace with prefixes [backend/]. ";
    prompt += "Empty failure_groups is valid only when passed=true. Never put tests/ paths in prefixes.\n";
    prompt += "Only after the full sequence is green, update docs/service-layering-remediation-contract.md for closed_ids: status DONE, ";
    prompt += "执行登记 with batch/owner/deps/evidence, and append a 批次验收登记 covering layering result, targeted unit tests, ";
    prompt += "Mongo N/A, unified unit gates, compat/rollback, and unrun external bounds.\n";
    prompt += "Do not mark BLOCKED rows DONE. Do not close SALES-E19 without SALES-R02..R05. Do not invent occupancy policy.\n";
    prompt += "closed_ids MUST be a subset of the Verify-passed allowed IDs listed above. held_ids are attempted but not closed.\n";
    prompt += "Do not git add or git commit. Leave this batch\u0027s code and contract edits in the working tree.\n";
    prompt
}

fn quality_repair_prompt_for_group(batch_id, group, quality_payload, attempt, stall, target_dir, preserve_note) {
    let prompt = "";
    prompt += "Repair only this unit-gate failure group for service-layering batch " + batch_id + ". Attempt " + attempt.to_string() + ".\n";
    prompt += "Group: " + group.name + ". Allowed prefixes: " + json_encode(group.prefixes) + ".\n";
    prompt += "Issues to fix: " + json_encode(group.issues) + "\n";
    prompt += "Failing commands for this group: " + json_encode(group.commands) + "\n";
    prompt += "Full Quality payload (context only): " + quality_payload + "\n";
    prompt += preserve_note + "\n";
    prompt += "Inspect git status and diff first. Never git reset/checkout/restore/clean. Preserve unrelated dirty files.\n";
    prompt += "PARALLEL SAFETY: other repairers may be editing disjoint prefixes at the same time. ";
    prompt += "Modify only files under the allowed prefixes. Format only touched files. Never cargo fmt --all. ";
    prompt += "Do not edit files under any tests/ directory.\n";
    prompt += "Fix only concrete gate breakage: fmt, check, clippy, --lib unit test failures, boundary scripts, or diff check. ";
    prompt += "Keep repository/entity/service/BPM boundaries from contract section 2. Old Service helpers must stay deleted.\n";
    prompt += "UNIT-ONLY: never add, modify, or run integration tests to make the gate green. ";
    prompt += "Forbidden: tests/ targets, cargo test without --lib, --test <name>, --include-ignored, MongoDB, or external services. ";
    prompt += "Fix with unit-testable code only. After edits, cargo fmt the touched files, then focused cargo test -p <crate> --lib with CARGO_TARGET_DIR=" + target_dir + ". ";
    prompt += "When finished, rm -rf " + target_dir + ". Do not run the workspace suite; Quality retests next.\n";
    if stall {
        prompt += "STALL: previous repair left the same failing fingerprint. Do not repeat the same patch. ";
        prompt += "Re-run the failing --lib test yourself, read the actual assertion/compile error, and fix the root cause.\n";
    }
    prompt += "Do not edit docs/service-layering-remediation-contract.md close statuses. Do not git add or git commit.\n";
    prompt
}

fn commit_prompt_for(batch_id, closed_ids, quality_payload, attempt) {
    let prompt = "";
    prompt += "Create exactly one git commit for this service-layering batch.\n";
    prompt += "Batch id: " + batch_id + ". Closed IDs: " + json_encode(closed_ids) + ". Attempt " + attempt.to_string() + ".\n";
    prompt += "Quality payload: " + quality_payload + "\n";
    prompt += "Inspect git status and diff first. Commit only this batch's files: Quality files_changed plus docs/service-layering-remediation-contract.md if this batch updated it, plus matching in-crate unit tests. Never stage crate tests/ integration tests.\n";
    prompt += "Do not stage unrelated dirty or untracked files. Record any left-unstaged paths in skipped_dirty.\n";
    prompt += "Never git reset, checkout, restore, clean, stash, rebase, merge, or push. Never amend a commit you did not just create in this step.\n";
    prompt += "If this batch has no staged changes after the scoped add, set committed=false and commit_sha=none. Do not create an empty commit.\n";
    prompt += "If unrelated dirty files block a clean scoped commit, set committed=false rather than scooping them in.\n";
    prompt += "Use a HEREDOC commit message in this repo's style, for example:\n";
    prompt += "git commit -m \"$(cat <<'EOF'\n";
    prompt += "refactor(backend): 完成 Service 分层整改批次 " + batch_id + "\n";
    prompt += "\n";
    prompt += "关闭 " + json_encode(closed_ids) + "。\n";
    prompt += "EOF\n";
    prompt += ")\"\n";
    prompt += "After a successful commit, commit_sha must be `git rev-parse HEAD`. If not committed, commit_sha must be the string none.\n";
    prompt
}

fn collect_pending(selected, work_ids, max_groups) {
    let pending = [];
    for wave in selected {
        for cluster in wave.clusters {
            let leftover = intersect_ids(cluster.item_ids, work_ids);
            if leftover.len() > 0 {
                if pending.len() < max_groups {
                    pending.push(#{
                        name: cluster.name,
                        item_ids: leftover,
                        prefixes: cluster.prefixes,
                        needs_mongo: cluster.needs_mongo,
                        wave_id: wave.id,
                    });
                } else {
                    log("Deferred extra cluster " + cluster.name + " in " + wave.id + " (max_groups=" + max_groups.to_string() + ")");
                }
            }
        }
    }
    pending
}

let waves = catalog_waves();
let catalog_ids = catalog_item_ids(waves);

let list_waves = false;
let dry_run = false;
let wave_arg = "next";
let max_waves = 8;
let max_groups = 8;
let max_parallel = 8;
let include_blocked = false;
let allow_p1 = false;
let max_quality_repairs = 12;
let max_repair_parallel = 4;
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
    if args.max_waves != () { max_waves = integer_or(args.max_waves, 8); }
    if args.max_groups != () {
        max_groups = integer_or(args.max_groups, 8);
    } else if args.max_clusters != () {
        max_groups = integer_or(args.max_clusters, 8);
    }
    if args.max_parallel != () { max_parallel = integer_or(args.max_parallel, 8); }
    if args.max_quality_repairs != () { max_quality_repairs = integer_or(args.max_quality_repairs, 12); }
    if args.max_repair_parallel != () { max_repair_parallel = integer_or(args.max_repair_parallel, 4); }
    if args.include_blocked != () { include_blocked = is_true(args.include_blocked); }
    if args.allow_p1_with_open_p0 != () { allow_p1 = is_true(args.allow_p1_with_open_p0); }
    if args.commit != () { do_commit = is_true(args.commit); }
    if is_text_array(args.item_ids) { custom_ids = args.item_ids; }
    if is_text(args.batch_date) { batch_date = args.batch_date; }
    if is_text(args.dirty_note) { dirty_note = args.dirty_note; }
}

if max_waves < 1 { max_waves = 1; }
if max_waves > 12 { max_waves = 12; }
if max_groups < 1 { max_groups = 1; }
if max_groups > 16 { max_groups = 16; }
if max_parallel < 1 { max_parallel = 1; }
if max_parallel > 16 { max_parallel = 16; }
if max_quality_repairs < 1 { max_quality_repairs = 1; }
if max_quality_repairs > 24 { max_quality_repairs = 24; }
if max_repair_parallel < 1 { max_repair_parallel = 1; }
if max_repair_parallel > 8 { max_repair_parallel = 8; }

if smoke_merge {
    let overlap = find_wave(waves, "p0-proc-po-plan");
    let indep = find_wave(waves, "p0-sales-independent");
    let overlap_groups = [];
    let indep_groups = [];
    if overlap != () { overlap_groups = merge_overlapping_clusters(overlap.clusters); }
    if indep != () { indep_groups = merge_overlapping_clusters(indep.clusters); }
    let overlap_slices = pack_index_slices(overlap_groups, max_parallel);
    let indep_slices = pack_index_slices(indep_groups, max_parallel);
    let overlap_names = [];
    for group in overlap_groups { overlap_names.push(group.name); }
    let indep_names = [];
    for group in indep_groups { indep_names.push(group.name); }
    let dummy_repair = [
        #{ name: "entities", prefixes: ["backend/entities/src/"], issues: ["a"], commands: [] },
        #{ name: "services", prefixes: ["backend/services/src/"], issues: ["b"], commands: [] },
        #{ name: "sales", prefixes: ["backend/services/src/sales/"], issues: ["c"], commands: [] },
        #{ name: "scripts", prefixes: ["backend/scripts/"], issues: ["d"], commands: [] },
    ];
    let dummy_repair_slices = pack_repair_slices(dummy_repair, 4);
    let lib_flagged = commands_ran_integration(["cargo test --workspace --lib --all-features --locked"]);
    let workspace_flagged = commands_ran_integration(["cargo test --workspace --all-features --locked"]);
    let mongo_flagged = commands_ran_integration(["ERP_TEST_MONGO_URI=x cargo test -p database --test foo"]);
    let cleaned_prefixes = sanitize_repair_prefixes(["backend/entities/src/", "backend/services/src/tests/", "docs/a", "/tmp/x"]);
    complete(#{
        summary: "overlap groups=" + overlap_groups.len().to_string() + " slices=" + overlap_slices.len().to_string() + "; indep groups=" + indep_groups.len().to_string() + " slices=" + indep_slices.len().to_string() + "; repair slices=" + dummy_repair_slices.len().to_string(),
        overlap_groups: overlap_names,
        overlap_slices: slice_preview(overlap_groups, overlap_slices),
        indep_groups: indep_names,
        indep_slices: slice_preview(indep_groups, indep_slices),
        repair_slices: slice_preview(dummy_repair, dummy_repair_slices),
        lib_flagged: lib_flagged,
        workspace_flagged: workspace_flagged,
        mongo_flagged: mongo_flagged,
        cleaned_prefixes: cleaned_prefixes,
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
    let preview_clusters = [];
    for wave in waves {
        let leftover = intersect_ids(ids_in_wave(wave), work_ids);
        if leftover.len() > 0 {
            remaining_waves.push(#{
                id: wave.id,
                kind: wave.kind,
                title: wave.title,
                remaining_ids: leftover,
            });
            for cluster in wave.clusters {
                let cluster_left = intersect_ids(cluster.item_ids, work_ids);
                if cluster_left.len() > 0 && preview_clusters.len() < max_groups {
                    preview_clusters.push(#{
                        name: cluster.name,
                        item_ids: cluster_left,
                        prefixes: cluster.prefixes,
                        needs_mongo: cluster.needs_mongo,
                        wave_id: wave.id,
                    });
                }
            }
        }
    }
    let preview_groups = merge_overlapping_clusters(preview_clusters);
    let preview_slices = pack_index_slices(preview_groups, max_parallel);
    let report = "# Service layering remaining waves\n\n";
    report += "- OPEN (executable): " + open_ids.len().to_string() + "\n";
    report += "- BLOCKED: " + json_encode(blocked_ids) + "\n";
    report += "- P0 still open: " + p0_still_open.to_string() + "\n";
    report += "- Catalog waves with remaining work: " + remaining_waves.len().to_string() + "\n";
    report += "- Packed groups this run (max_groups=" + max_groups.to_string() + "): " + preview_groups.len().to_string() + "\n";
    report += "- Parallel slices: " + preview_slices.len().to_string() + " (max_parallel=" + max_parallel.to_string() + ")\n\n";
    report += "```json\n" + json_encode(#{ waves: remaining_waves, slices: slice_preview(preview_groups, preview_slices) }) + "\n```\n";
    let report_path = write_scratch_file("layering-remaining.md", report);
    complete(#{
        summary: remaining_waves.len().to_string() + " waves still have OPEN items; packed " + preview_groups.len().to_string() + " groups into " + preview_slices.len().to_string() + " parallel slices",
        report_path: report_path,
        open_count: open_ids.len(),
        blocked_ids: blocked_ids,
        remaining_waves: remaining_waves,
        slices: slice_preview(preview_groups, preview_slices),
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
    if intersect_ids(ids_in_wave(found), work_ids).len() == 0 {
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
            if kind_ok && intersect_ids(ids_in_wave(wave), work_ids).len() > 0 {
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

let pending = collect_pending(selected, work_ids, max_groups);
if pending.len() == 0 {
    complete(#{
        summary: "Selected waves had no remaining OPEN clusters after max_groups filter",
        waves: selected_names,
    });
}

let plan_cap = remaining_budget();
if plan_cap > 6 {
    plan_cap = (plan_cap - 6) / 4;
} else {
    plan_cap = 0;
}
if plan_cap < 1 {
    pause("infra", "Remaining agent budget is too low to plan and verify a cluster. Resume with a higher agent_budget.");
}
if pending.len() > plan_cap {
    log("Shrinking pending clusters from " + pending.len().to_string() + " to " + plan_cap.to_string() + " for budget");
    pending = take_prefix(pending, plan_cap);
}

phase("Plan");
let plan_jobs = [];
for cluster in pending {
    plan_jobs.push(#{
        prompt: plan_prompt_for(cluster),
        label: "plan:" + cluster.name,
        capability_mode: "read-only",
        output_schema: cluster_plan_schema,
    });
}
let plan_results = launch_jobs_or_seq(plan_jobs, 6);

let active_clusters = [];
let plan_index = 0;
for cluster in pending {
    let matched = ();
    if plan_index < plan_results.len() { matched = plan_results[plan_index]; }
    let skip_reason = "planner omitted cluster; fail closed";
    let still_open = false;
    let code_still_wrong = false;
    let needs_mongo = cluster.needs_mongo;
    let item_ids = cluster.item_ids;
    if matched != () && matched.success && matched.output != () {
        skip_reason = "";
        if is_text(matched.output.skip_reason) { skip_reason = matched.output.skip_reason; }
        if type_of(matched.output.still_open) == "bool" { still_open = matched.output.still_open; }
        if type_of(matched.output.code_still_wrong) == "bool" { code_still_wrong = matched.output.code_still_wrong; }
        if type_of(matched.output.needs_mongo) == "bool" { needs_mongo = matched.output.needs_mongo; }
        if is_text_array(matched.output.item_ids) {
            item_ids = intersect_ids(matched.output.item_ids, cluster.item_ids);
        }
    }
    if skip_reason == "" && still_open && code_still_wrong && item_ids.len() > 0 {
        active_clusters.push(#{
            name: cluster.name,
            item_ids: item_ids,
            prefixes: cluster.prefixes,
            needs_mongo: needs_mongo,
            wave_id: cluster.wave_id,
        });
    } else {
        if skip_reason == "" { skip_reason = "not still_open+code_still_wrong"; }
        log("Skipping cluster " + cluster.name + ": " + skip_reason);
    }
    plan_index += 1;
}

if active_clusters.len() == 0 {
    complete(#{
        summary: "Planner found nothing to implement in the packed waves",
        waves: selected_names,
    });
}

let impl_groups = merge_overlapping_clusters(active_clusters);
let impl_cap = remaining_budget();
if impl_cap > 6 {
    impl_cap = (impl_cap - 6) / 3;
} else {
    impl_cap = 0;
}
if impl_cap < 1 {
    pause("infra", "Remaining agent budget is too low to implement and verify. Resume with a higher agent_budget.");
}
if impl_groups.len() > impl_cap {
    log("Shrinking implement groups from " + impl_groups.len().to_string() + " to " + impl_cap.to_string() + " for budget");
    impl_groups = take_prefix(impl_groups, impl_cap);
}

let merged_names = [];
for group in impl_groups { merged_names.push(group.name); }
let impl_slices = pack_index_slices(impl_groups, max_parallel);
log("Implement groups: " + json_encode(merged_names));
log("Parallel slices: " + json_encode(slice_preview(impl_groups, impl_slices)));

let batch_id = batch_date + "-layering";
let implementation_results = [];
for group in impl_groups {
    implementation_results.push(empty_impl(group, "not started"));
}

phase("Implement");
let slice_no = 0;
for slice in impl_slices {
    let jobs = [];
    for idx in slice {
        if idx < impl_groups.len() {
            let cluster = impl_groups[idx];
            let target_dir = "/tmp/erp-layering-" + idx.to_string();
            jobs.push(#{
                prompt: implement_prompt_for(cluster, batch_id, dirty_note, target_dir),
                label: "implement:" + cluster.name,
                capability_mode: "all",
                output_schema: implementation_schema,
            });
        }
    }
    log("Implement slice " + slice_no.to_string() + ": " + json_encode(names_for_slice(impl_groups, slice)));
    let raw = launch_jobs_or_seq(jobs, 4);
    let ji = 0;
    for idx in slice {
        let slot = ();
        if ji < raw.len() { slot = raw[ji]; }
        if idx < impl_groups.len() {
            implementation_results[idx] = impl_from_agent(impl_groups[idx], slot);
        }
        ji += 1;
    }
    slice_no += 1;
}

let successful_impls = 0;
for row in implementation_results {
    if is_true(row.success) { successful_impls += 1; }
}
if successful_impls == 0 {
    complete(#{
        summary: "All implementers failed; nothing to review or close",
        waves: selected_names,
        groups: merged_names,
        quality_passed_all: false,
    });
}

phase("Review");
let review_jobs = [];
let review_indexes = [];
let ri = 0;
for cluster in impl_groups {
    if ri < implementation_results.len() {
        if is_true(implementation_results[ri].success) {
            review_jobs.push(#{
                prompt: review_prompt_for(cluster, json_encode(implementation_results[ri])),
                label: "review:" + cluster.name,
                capability_mode: "read-only",
                output_schema: review_schema,
            });
            review_indexes.push(ri);
        }
    }
    ri += 1;
}
let review_raw = launch_jobs_or_seq(review_jobs, 4);
let reviews_by_group = [];
for cluster in impl_groups { reviews_by_group.push(()); }
let rp = 0;
for mapped in review_indexes {
    if rp < review_raw.len() { reviews_by_group[mapped] = review_raw[rp]; }
    rp += 1;
}

phase("Repair");
let repair_needed = [];
let gi = 0;
for cluster in impl_groups {
    let needs = false;
    if gi < implementation_results.len() {
        if is_true(implementation_results[gi].success) {
            let review = ();
            if gi < reviews_by_group.len() { review = reviews_by_group[gi]; }
            if !review_passed(review) { needs = true; }
        }
    }
    if needs { repair_needed.push(gi); }
    gi += 1;
}
let repair_groups = [];
for idx in repair_needed {
    if idx < impl_groups.len() { repair_groups.push(impl_groups[idx]); }
}
let repair_slices = pack_index_slices(repair_groups, max_parallel);
let repair_slice_no = 0;
for slice in repair_slices {
    let jobs = [];
    let slice_global = [];
    for local_idx in slice {
        if local_idx < repair_needed.len() {
            let gi2 = repair_needed[local_idx];
            if gi2 < impl_groups.len() {
                let cluster = impl_groups[gi2];
                let review = ();
                if gi2 < reviews_by_group.len() { review = reviews_by_group[gi2]; }
                let findings = review_findings_payload(review);
                let agent_id = "";
                if gi2 < implementation_results.len() { agent_id = implementation_results[gi2].agent_id; }
                let resumed = is_text(agent_id);
                let target_dir = "/tmp/erp-layering-r" + gi2.to_string();
                let job = #{
                    prompt: repair_prompt_for(cluster, findings, dirty_note, resumed, target_dir),
                    label: "repair:" + cluster.name,
                    capability_mode: "all",
                    output_schema: implementation_schema,
                };
                if resumed { job.resume_from = agent_id; }
                jobs.push(job);
                slice_global.push(gi2);
            }
        }
    }
    log("Repair slice " + repair_slice_no.to_string() + " size=" + jobs.len().to_string());
    let raw = launch_jobs_or_seq(jobs, 3);
    let ji = 0;
    for gi2 in slice_global {
        let slot = ();
        if ji < raw.len() { slot = raw[ji]; }
        if gi2 < impl_groups.len() {
            if slot != () && slot.success && slot.output != () {
                implementation_results[gi2] = impl_from_agent(impl_groups[gi2], slot);
            }
        }
        ji += 1;
    }
    repair_slice_no += 1;
}

phase("Verify");
let verify_jobs = [];
let verify_indexes = [];
let vi = 0;
for cluster in impl_groups {
    if vi < implementation_results.len() {
        if is_true(implementation_results[vi].success) {
            let review_payload = "review unavailable";
            if vi < reviews_by_group.len() { review_payload = json_encode(reviews_by_group[vi]); }
            verify_jobs.push(#{
                prompt: verify_prompt_for(cluster, json_encode(implementation_results[vi]), review_payload),
                label: "verify:" + cluster.name,
                capability_mode: "read-only",
                output_schema: verify_schema,
            });
            verify_indexes.push(vi);
        }
    }
    vi += 1;
}
let verify_raw = launch_jobs_or_seq(verify_jobs, 2);
let verifies_by_group = [];
for cluster in impl_groups { verifies_by_group.push(()); }
let vp = 0;
for mapped in verify_indexes {
    if vp < verify_raw.len() { verifies_by_group[mapped] = verify_raw[vp]; }
    vp += 1;
}

let verified_ids = [];
let verify_held = [];
let vg = 0;
for cluster in impl_groups {
    let verdict = ();
    if vg < verifies_by_group.len() { verdict = verifies_by_group[vg]; }
    if verdict != () && verdict.success && verdict.output != () && is_true(verdict.output.passed) {
        if is_text_array(verdict.output.completed_ids) {
            let accepted = intersect_ids(verdict.output.completed_ids, cluster.item_ids);
            if vg < implementation_results.len() {
                accepted = intersect_ids(accepted, implementation_results[vg].completed_ids);
            }
            for item_id in accepted { verified_ids = add_unique(verified_ids, item_id); }
        }
        if is_text_array(verdict.output.held_ids) {
            for item_id in verdict.output.held_ids { verify_held = add_unique(verify_held, item_id); }
        }
    } else {
        for item_id in cluster.item_ids { verify_held = add_unique(verify_held, item_id); }
        log("Verify fail-closed for " + cluster.name);
    }
    vg += 1;
}
log("Verified IDs: " + json_encode(verified_ids));

let quality_ok = false;
let wave_closed = [];
let wave_held = verify_held;
let quality_result = ();
let quality_repair_count = 0;
let quality_stall_streak = 0;
let last_quality_fp = "";
let unit_tests_ok = false;
if verified_ids.len() == 0 {
    log("No Verify-passed IDs; skipping Quality and Commit");
} else {
    let quality_slots = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24];
    let stop_quality = false;
    for attempt in quality_slots {
        if !stop_quality {
            if attempt <= max_quality_repairs {
                if remaining_budget() < 2 {
                    log("Quality loop stopping: remaining budget=" + remaining_budget().to_string());
                    stop_quality = true;
                } else {
                    if attempt > 0 {
                        phase("QualityRepair");
                        let stall = false;
                        if quality_stall_streak >= 2 { stall = true; }
                        dispatch_quality_repairs(batch_id, quality_result, attempt, stall, dirty_note, max_repair_parallel);
                        quality_repair_count += 1;
                    }
                    if remaining_budget() < 1 {
                        log("Quality loop stopping before retest: remaining budget=0");
                        stop_quality = true;
                    } else {
                        phase("Quality");
                        let retest = false;
                        if attempt > 0 { retest = true; }
                        let qlabel = "quality:batch";
                        if retest { qlabel = "quality:batch-r" + attempt.to_string(); }
                        quality_result = agent(quality_prompt_for(
                            batch_id,
                            verified_ids,
                            json_encode(implementation_results),
                            json_encode(reviews_by_group),
                            json_encode(verifies_by_group),
                            retest
                        ), #{
                            label: qlabel,
                            capability_mode: "all",
                            output_schema: quality_schema,
                        });
                        if quality_passed(quality_result) {
                            quality_ok = true;
                            stop_quality = true;
                            log("Quality gate passed on attempt " + attempt.to_string() + " repairs=" + quality_repair_count.to_string());
                        } else {
                            let qfp = quality_fingerprint_of(quality_result);
                            if qfp == last_quality_fp {
                                if last_quality_fp != "" { quality_stall_streak += 1; }
                            } else {
                                quality_stall_streak = 0;
                            }
                            last_quality_fp = qfp;
                            log("Quality gate failed attempt=" + attempt.to_string() + " stall=" + quality_stall_streak.to_string());
                        }
                    }
                }
            }
        }
    }
    if quality_ok {
        if quality_result != () && quality_result.success && quality_result.output != () {
            if is_text_array(quality_result.output.closed_ids) {
                wave_closed = intersect_ids(quality_result.output.closed_ids, verified_ids);
            }
        }
    }
    if quality_result != () && quality_result.success && quality_result.output != () {
        unit_tests_ok = is_true(quality_result.output.unit_tests_passed);
        if is_text_array(quality_result.output.held_ids) {
            for item_id in quality_result.output.held_ids { wave_held = add_unique(wave_held, item_id); }
        }
    }
    log("Quality closed loop finished: passed=" + quality_ok.to_string() + " unit_tests=" + unit_tests_ok.to_string() + " repairs=" + quality_repair_count.to_string());
}

let commit_output = #{
    committed: false,
    commit_sha: "none",
    files_staged: [],
    skipped_dirty: [],
    summary: "commit skipped",
};
if do_commit && quality_ok && wave_closed.len() > 0 {
    let quality_payload = "quality unavailable";
    if quality_result != () { quality_payload = json_encode(quality_result); }
    let commit_slots = [0, 1, 2];
    for ci in commit_slots {
        if !is_true(commit_output.committed) {
            if remaining_budget() < 1 {
                commit_output.summary = "commit skipped: no agent budget";
            } else {
                phase("Commit");
                let attempt_no = ci + 1;
                let commit_result = agent(commit_prompt_for(batch_id, wave_closed, quality_payload, attempt_no), #{
                    label: "commit:batch-r" + attempt_no.to_string(),
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
                } else {
                    commit_output.summary = "commit agent failed on attempt " + attempt_no.to_string();
                }
            }
        }
    }
} else if !do_commit {
    commit_output.summary = "args.commit=false";
} else if !quality_ok {
    commit_output.summary = "quality did not pass; left uncommitted";
} else {
    commit_output.summary = "no closed_ids; left uncommitted";
}

if do_commit && quality_ok && wave_closed.len() > 0 && !is_true(commit_output.committed) {
    quality_ok = false;
}

work_ids = subtract_ids(work_ids, wave_closed);

phase("Report");
let report = "# Service layering remediation run\n\n";
report += "- Wave arg: " + wave_arg + "\n";
report += "- Selected waves: " + json_encode(selected_names) + "\n";
report += "- Packed groups: " + json_encode(merged_names) + "\n";
report += "- Parallel slices: " + json_encode(slice_preview(impl_groups, impl_slices)) + "\n";
report += "- Verified IDs (close candidates): " + json_encode(verified_ids) + "\n";
report += "- Closed this run: " + json_encode(wave_closed) + "\n";
report += "- Held: " + json_encode(wave_held) + "\n";
report += "- Quality passed: " + quality_ok.to_string() + "\n";
report += "- Unit tests passed (--lib only): " + unit_tests_ok.to_string() + "\n";
report += "- Quality repair rounds: " + quality_repair_count.to_string() + "\n";
report += "- Quality stall streak: " + quality_stall_streak.to_string() + "\n";
report += "- Commit: " + json_encode(commit_output) + "\n";
report += "- Remaining executable OPEN after run (inventory minus closed): " + work_ids.len().to_string() + "\n";
report += "- BLOCKED still excluded: " + json_encode(blocked_ids) + "\n\n";
report += "Gate is unit-only: cargo test --workspace --lib. Integration tests are forbidden. ";
report += "Failed gates shard into disjoint prefix groups and repair in parallel, then retest until green or budget is exhausted. ";
report += "Next: re-run with args.wave=next until P0 slices are empty, then P1. ";
report += "Tune args.max_groups / args.max_parallel / args.max_waves / args.max_quality_repairs / args.max_repair_parallel. ";
report += "Same-prefix clusters stay sequential slices. ";
report += "Do not implement blocked-returns-occupancy until finance owners sign occupancy policy.\n\n";
report += "## Implementer results\n\n```json\n" + json_encode(implementation_results) + "\n```\n";
let report_path = write_scratch_file("service-layering-remediation-report.md", report);

let commits_total = [];
if is_true(commit_output.committed) && commit_output.commit_sha != "none" {
    commits_total.push(#{ batch: batch_id, commit_sha: commit_output.commit_sha });
}

complete(#{
    summary: wave_closed.len().to_string() + " IDs closed, " + wave_held.len().to_string() + " held, slices=" + impl_slices.len().to_string() + ", groups=" + impl_groups.len().to_string() + ", quality_passed=" + quality_ok.to_string() + ", unit_tests=" + unit_tests_ok.to_string() + ", repairs=" + quality_repair_count.to_string(),
    report_path: report_path,
    closed_ids: wave_closed,
    held_ids: wave_held,
    verified_ids: verified_ids,
    commits: commits_total,
    quality_passed_all: quality_ok,
    unit_tests_passed: unit_tests_ok,
    quality_repair_rounds: quality_repair_count,
    remaining_open_estimate: work_ids.len(),
    slices: slice_preview(impl_groups, impl_slices),
    groups: merged_names,
    waves: selected_names,
});
