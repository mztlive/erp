// Normalize all docs under docs/ into instructional, conclusive, contract-style prose.
// Removes discussion / retrospective / process scaffolding.
//
// Run: /workflow docs-contract-normalize
// Args (all optional):
//   root: docs root relative to repo (default "docs")
//   only: array of relative paths to limit scope (default: all known md files)
//   skip_verify: bool (default false)
//   dry_run: bool (default false) — inventory only, no edits

let meta = #{
    name: "docs-contract-normalize",
    description: "Strip discussion/retrospective/process prose from docs; keep instructional, conclusive, contract language",
    when_to_use: "When ERP docs need to be pure contracts: no open questions, no rationale essays, no process notes",
    phases: [
        #{ title: "Inventory", detail: "Scan docs for non-contract prose" },
        #{ title: "Normalize", detail: "Parallel rewrite each file in place" },
        #{ title: "Verify", detail: "Adversarial check residual process language" },
        #{ title: "Report", detail: "Write summary artifact" },
    ],
};

// --- args ---
let root = "docs";
let skip_verify = false;
let dry_run = false;
let only_paths = [];
if args != () {
    if args.root != () { root = args.root; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
    if args.dry_run != () { dry_run = args.dry_run; }
    if args.only != () { only_paths = args.only; }
}

// Deterministic work list — full docs corpus (no discovery agent).
let all_files = [
    "docs/erp-data-model.md",
    "docs/erp-mall-data-mapping.md",
    "docs/erp-phase-1.md",
    "docs/erp-phase-2.md",
    "docs/erp-ui-design.md",
    "docs/erp-ui-flows.md",
    "docs/ui-glossary.md",
    "docs/ui-workspaces/README.md",
    "docs/ui-workspaces/w01-today-workspace.md",
    "docs/ui-workspaces/w02-unified-task-queue.md",
    "docs/ui-workspaces/w03-customer-center.md",
    "docs/ui-workspaces/w04-contracts.md",
    "docs/ui-workspaces/w05-sales-orders.md",
    "docs/ui-workspaces/w06-customer-acceptance.md",
    "docs/ui-workspaces/w07-procurement-confirmation-queue.md",
    "docs/ui-workspaces/w08-purchase-orders.md",
    "docs/ui-workspaces/w10-inventory-ledger.md",
    "docs/ui-workspaces/w11-customer-receivables.md",
    "docs/ui-workspaces/w12-supplier-payables.md",
    "docs/ui-workspaces/w13-card-funds-review.md",
    "docs/ui-workspaces/w14-basic-data.md",
    "docs/ui-workspaces/w15-customer-business-quality.md",
    "docs/ui-workspaces/w16-actual-profit-loss.md",
    "docs/ui-workspaces/w17-mall-sync-mapping.md",
    "docs/ui-workspaces/w18-import-opening.md",
    "docs/ui-workspaces/w19-permissions-audit.md",
    "docs/ui-workspaces/w20-supplier-api-connections.md",
    "docs/ui-workspaces/w21-supplier-catalog.md",
    "docs/ui-workspaces/w22-product-publication.md",
    "docs/ui-workspaces/w23-execution-projection.md",
    "docs/ui-workspaces/w25-mall-consumption-orders.md",
    "docs/ui-workspaces/w26-supplier-orders.md",
    "docs/ui-workspaces/w27-api-settlement.md",
    "docs/ui-workspaces/w28-card-consumption-analytics.md",
    "docs/ui-workspaces/w29-integration-error-reconciliation.md",
    "docs/ui-workspaces/w30-historical-consumption-backfill.md",
];

let files = [];
if only_paths.len() > 0 {
    for p in only_paths {
        // Only allow paths already in the known corpus.
        let allowed = false;
        for a in all_files {
            if a == p { allowed = true; }
        }
        if allowed {
            files.push(p);
        } else {
            log("drop out-of-corpus path: " + p);
        }
    }
} else {
    files = all_files;
}

if files.len() == 0 {
    pause("verification", "No files to process. Pass args.only as array of docs/*.md paths, or omit only for full corpus.");
}

log("docs-contract-normalize: " + files.len().to_string() + " files; dry_run=" + dry_run.to_string());

// --- schemas ---
let inventory_schema = #{
    "type": "object",
    "required": ["files", "summary"],
    "properties": #{
        "summary": #{ "type": "string" },
        "files": #{
            "type": "array",
            "items": #{
                "type": "object",
                "required": ["path", "severity", "issues"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "severity": #{ "type": "string" },
                    "issues": #{
                        "type": "array",
                        "maxItems": 20,
                        "items": #{ "type": "string" },
                    },
                    "has_open_questions": #{ "type": "boolean" },
                    "has_rationale": #{ "type": "boolean" },
                    "has_process_meta": #{ "type": "boolean" },
                },
            },
        },
    },
};

let rewrite_schema = #{
    "type": "object",
    "required": ["path", "changed", "actions", "summary"],
    "properties": #{
        "path": #{ "type": "string" },
        "changed": #{ "type": "boolean" },
        "actions": #{
            "type": "array",
            "maxItems": 30,
            "items": #{ "type": "string" },
        },
        "promoted_rules": #{
            "type": "array",
            "maxItems": 20,
            "items": #{ "type": "string" },
        },
        "removed_sections": #{
            "type": "array",
            "maxItems": 20,
            "items": #{ "type": "string" },
        },
        "summary": #{ "type": "string" },
        "skipped_reason": #{ "type": "string" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["path", "pass", "residual_issues", "notes"],
    "properties": #{
        "path": #{ "type": "string" },
        "pass": #{ "type": "boolean" },
        "residual_issues": #{
            "type": "array",
            "maxItems": 15,
            "items": #{
                "type": "object",
                "required": ["kind", "snippet"],
                "properties": #{
                    "kind": #{ "type": "string" },
                    "snippet": #{ "type": "string" },
                    "line_hint": #{ "type": "string" },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

// Shared style contract embedded in prompts (kept short; agents re-read file).
let style_rules = "";
style_rules += "目标文风（必须同时满足）：\n";
style_rules += "1) 指导性：直接写做什么、怎么做、禁止什么、成功/失败结果；可验收。\n";
style_rules += "2) 结论性：只写已定规则，不写争论、取舍过程、备选方案。\n";
style_rules += "3) 合同式：用「必须 / 禁止 / 仅当 / 否则 / 不得」；禁「建议先」「实现阶段再定」「可选（未决）」。\n";
style_rules += "\n必须删除或改写的非合同内容：\n";
style_rules += "A. 讨论式：标题/段落「为什么…」、优劣对比、开放讨论、未来单独讨论。\n";
style_rules += "B. 复盘式：变更史叙事、演进说明、评审纪要、决策过程（保留决策结论本身）。\n";
style_rules += "C. 过程式：「待确认事项」表（问题/建议决策人/当前建议）、WIP、TBD、实现阶段与后端确认。\n";
style_rules += "\n对「§13 待确认事项」或同类开放问题：\n";
style_rules += "- 若有「当前建议」：把建议提升为正文正式规则（必须/禁止），写回相关章节，删除问题表。\n";
style_rules += "- 正文若写「见待确认事项 Qx」：改为直接写正式规则，去掉交叉引用。\n";
style_rules += "- 若无建议、纯开放：删除问题；相关能力采用保守规则（禁用/只读/不提供），不发明新业务状态机。\n";
style_rules += "- 删除「确认后应写回…」这类过程说明。\n";
style_rules += "\n对「未来单独讨论 / 不在本期」：改写为明确范围声明「不建设 / 不在范围内」，不是开放议题。\n";
style_rules += "对决策索引表：保留结论与出处；措辞改为合同式（不写「未来单独讨论」）。\n";
style_rules += "\n保留且不得破坏：\n";
style_rules += "- 业务状态机、金额/权限/来源口径、路由、字段、验收清单、实体不变量。\n";
style_rules += "- 业务实体名「草稿」状态、合法业务词（待确认任务状态等）——不要误删业务语义。\n";
style_rules += "- Markdown 结构层级、表格、交叉文档引用（除非引用的是已删除的过程章节）。\n";
style_rules += "- 文档头元数据可保留「状态 / 路由 / 角色」；去掉变更日志式「最后更新：…（长叙事）」中的过程叙事，可留日期。\n";
style_rules += "\n编辑纪律：\n";
style_rules += "- 使用 read_file / grep 读真实文件，禁止凭记忆整篇重写。\n";
style_rules += "- 用 search_replace 做就地修改；大文件只改命中段落。\n";
style_rules += "- 不得新建无关文件；不得改代码；只改指定 markdown。\n";
style_rules += "- 不得编造文档中不存在的业务规则；提升规则只能来自「当前建议」或已有正文。\n";

// =============================================================================
phase("Inventory");
// =============================================================================

let inv_prompt = "";
inv_prompt += "你是文档审计员。只读审查下列 ERP 文档路径是否含讨论式/复盘式/过程式内容。\n";
inv_prompt += "工作区根目录即仓库根。对每个文件：用 read_file 或 grep 实际打开，不要凭记忆。\n";
inv_prompt += "文件列表（JSON）：\n";
inv_prompt += json_encode(files);
inv_prompt += "\n\n判定 severity：\n";
inv_prompt += "- high：有「待确认事项」表、为什么…专节、未来单独讨论、实现阶段未决\n";
inv_prompt += "- medium：散落建议/可选未决/过程元数据\n";
inv_prompt += "- low：仅轻微措辞，整体已是合同式\n";
inv_prompt += "- clean：无需改动\n";
inv_prompt += "issues 用中文短句列出具体段落/标题。has_open_questions/has_rationale/has_process_meta 如实打标。\n";
inv_prompt += "summary 一段话概括全库主要问题类型。\n";

let inv = agent(inv_prompt, #{
    label: "inventory",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});

let inventory_ok = inv != () && inv.success && inv.output.files != ();
if !inventory_ok {
    log("inventory failed or empty; continue with full file list at medium severity");
} else {
    log("inventory: " + inv.output.summary);
}

if dry_run {
    let report = "# docs-contract-normalize dry_run inventory\n\n";
    if inventory_ok {
        report += inv.output.summary + "\n\n";
        report += "```json\n" + json_encode(inv.output.files) + "\n```\n";
    } else {
        report += "Inventory agent failed; would process " + files.len().to_string() + " files.\n";
    }
    let p = write_scratch_file("inventory.md", report);
    complete(#{ mode: "dry_run", path: p, file_count: files.len() });
}

// =============================================================================
phase("Normalize");
// =============================================================================

let rewrite_jobs = [];
for fpath in files {
    let sev = "medium";
    let hints = [];
    if inventory_ok {
        for item in inv.output.files {
            if item.path == fpath {
                if item.severity != () { sev = item.severity; }
                if item.issues != () { hints = item.issues; }
            }
        }
    }

    let rprompt = "";
    rprompt += "你是 ERP 文档合同化编辑器。就地改写下列文件，使其成为指导性、结论性、合同式文档。\n";
    rprompt += "目标文件（唯一允许修改）：" + fpath + "\n";
    rprompt += "库存审计 severity=" + sev + "；已知 issues（可能不全，仍须通读文件）：\n";
    rprompt += json_encode(hints);
    rprompt += "\n\n";
    rprompt += style_rules;
    rprompt += "\n\n步骤：\n";
    rprompt += "1. read_file 分段阅读全文（大文件按 offset/limit 分块）。\n";
    rprompt += "2. grep 搜：待确认|为什么|建议决策|当前建议|未来单独讨论|实现阶段|开放问题|WIP|TBD|评审时|决策过程|变更记录。\n";
    rprompt += "3. 按 style_rules 用 search_replace 修改；可多次替换。\n";
    rprompt += "4. 修改后重读关键段落，确认无残留过程表，且业务规则未丢失。\n";
    rprompt += "5. 若文件已是合同式且无命中，changed=false 并说明 skipped_reason。\n";
    rprompt += "输出 path/changed/actions/promoted_rules/removed_sections/summary。\n";

    rewrite_jobs.push(#{
        prompt: rprompt,
        label: "norm:" + fpath,
        capability_mode: "read-write",
        output_schema: rewrite_schema,
    });
}

let rewrite_results = parallel(rewrite_jobs);

let rewrite_ok = [];
let rewrite_fail = [];
let rewritten_paths = [];
let ri = 0;
for rr in rewrite_results {
    let fpath = files[ri];
    if rr != () && rr.success && rr.output != () {
        rewrite_ok.push(rr.output);
        if rr.output.changed == true {
            rewritten_paths.push(fpath);
        }
        log("rewrite " + fpath + " changed=" + rr.output.changed.to_string());
    } else {
        rewrite_fail.push(fpath);
        log("rewrite FAILED: " + fpath);
    }
    ri += 1;
}

log("rewrite done: ok=" + rewrite_ok.len().to_string() + " fail=" + rewrite_fail.len().to_string() + " changed=" + rewritten_paths.len().to_string());

// =============================================================================
phase("Verify");
// =============================================================================

let verify_results = [];
if skip_verify {
    log("skip_verify=true; skipping adversarial verify");
} else {
    // Verify every file that was processed (changed or not) for residual process language.
    let vjobs = [];
    for fpath in files {
        let vprompt = "";
        vprompt += "你是对抗性文档审查员。只读检查文件是否仍含讨论式/复盘式/过程式内容。\n";
        vprompt += "文件：" + fpath + "\n";
        vprompt += "必须用 read_file/grep 实际打开文件。\n";
        vprompt += "判定 residual（命中则记入 residual_issues，kind 取 discussion|retrospective|process|soft_language）：\n";
        vprompt += "- 「待确认事项」问题表（问题/建议决策人/当前建议）\n";
        vprompt += "- 「为什么…」专节或决策过程叙事\n";
        vprompt += "- 「未来单独讨论」开放议题措辞\n";
        vprompt += "- 「实现阶段再定/与后端确认/建议先」等未决过程语\n";
        vprompt += "NOT residual（不算问题）：\n";
        vprompt += "- 业务状态「草稿」「待确认」\n";
        vprompt += "- 明确的「不建设 / 不在范围内」范围声明\n";
        vprompt += "- 验收清单、权限表、必须/禁止规则\n";
        vprompt += "pass=true 仅当 residual_issues 为空。notes 简述整体是否合同式。\n";
        vjobs.push(#{
            prompt: vprompt,
            label: "verify:" + fpath,
            capability_mode: "read-only",
            output_schema: verify_schema,
        });
    }
    verify_results = parallel(vjobs);
}

// Optional fix round for failed verifies (bounded: only failed paths)
let fix_paths = [];
let vi = 0;
if !skip_verify {
    for vr in verify_results {
        let fpath = files[vi];
        if vr != () && vr.success && vr.output != () {
            if vr.output.pass != true {
                fix_paths.push(fpath);
                log("verify FAIL: " + fpath + " issues=" + json_encode(vr.output.residual_issues));
            }
        } else {
            // Fail closed: treat missing verify as needing fix pass only if we have budget
            log("verify missing/failed for: " + fpath);
        }
        vi += 1;
    }
}

if fix_paths.len() > 0 {
    log("fix round for " + fix_paths.len().to_string() + " files");
    let fjobs = [];
    for fpath in fix_paths {
        let residual = [];
        let vj = 0;
        for vr in verify_results {
            if files[vj] == fpath && vr != () && vr.success && vr.output != () {
                if vr.output.residual_issues != () {
                    residual = vr.output.residual_issues;
                }
            }
            vj += 1;
        }
        let fprompt = "";
        fprompt += "你是文档合同化修复编辑器。文件验证未通过，请就地修复。\n";
        fprompt += "目标文件：" + fpath + "\n";
        fprompt += "验证残留问题：\n";
        fprompt += json_encode(residual);
        fprompt += "\n\n";
        fprompt += style_rules;
        fprompt += "\n用 read_file/grep/search_replace 只改本文件。修复后输出 changed/actions/summary。\n";
        fjobs.push(#{
            prompt: fprompt,
            label: "fix:" + fpath,
            capability_mode: "read-write",
            output_schema: rewrite_schema,
        });
    }
    let fix_results = parallel(fjobs);
    for fr in fix_results {
        if fr != () && fr.success && fr.output != () {
            log("fix " + fr.output.path + " changed=" + fr.output.changed.to_string());
            if fr.output.changed == true {
                rewrite_ok.push(fr.output);
            }
        }
    }
}

// =============================================================================
phase("Report");
// =============================================================================

let report = "";
report += "# docs-contract-normalize 报告\n\n";
report += "目标：去除讨论式 / 复盘式 / 过程式内容，保留指导性、结论性、合同式正文。\n\n";
report += "## 范围\n\n";
report += "- 文件数：" + files.len().to_string() + "\n";
report += "- 改写成功输出：" + rewrite_ok.len().to_string() + "\n";
report += "- 改写失败：" + rewrite_fail.len().to_string() + "\n";
report += "- 首轮标记 changed 路径数：" + rewritten_paths.len().to_string() + "\n";
report += "- 验证失败后修复路径数：" + fix_paths.len().to_string() + "\n\n";

if inventory_ok {
    report += "## 库存摘要\n\n";
    report += inv.output.summary + "\n\n";
}

report += "## 改写结果\n\n";
for out in rewrite_ok {
    let ch = "no";
    if out.changed == true { ch = "yes"; }
    report += "### " + out.path + " (changed=" + ch + ")\n\n";
    report += out.summary + "\n\n";
    if out.actions != () {
        report += "Actions:\n";
        for a in out.actions {
            report += "- " + a + "\n";
        }
        report += "\n";
    }
    if out.promoted_rules != () {
        report += "Promoted rules:\n";
        for r in out.promoted_rules {
            report += "- " + r + "\n";
        }
        report += "\n";
    }
    if out.removed_sections != () {
        report += "Removed sections:\n";
        for s in out.removed_sections {
            report += "- " + s + "\n";
        }
        report += "\n";
    }
}

if rewrite_fail.len() > 0 {
    report += "## 改写失败文件\n\n";
    for f in rewrite_fail {
        report += "- " + f + "\n";
    }
    report += "\n";
}

if !skip_verify {
    report += "## 验证\n\n";
    let vpass = 0;
    let vfail = 0;
    let vmiss = 0;
    let vix = 0;
    for vr in verify_results {
        let fpath = files[vix];
        if vr != () && vr.success && vr.output != () {
            if vr.output.pass == true {
                vpass += 1;
            } else {
                vfail += 1;
                report += "- FAIL `" + fpath + "`: " + json_encode(vr.output.residual_issues) + "\n";
            }
        } else {
            vmiss += 1;
            report += "- MISSING `" + fpath + "`\n";
        }
        vix += 1;
    }
    report += "\npass=" + vpass.to_string() + " fail=" + vfail.to_string() + " missing=" + vmiss.to_string() + "\n";
    if fix_paths.len() > 0 {
        report += "\n已对 fail 路径执行一轮 fix；请人工抽查 fix 后文件。\n";
    }
}

report += "\n## 完成标准\n\n";
report += "- 文档正文无「待确认事项」问题表。\n";
report += "- 无「为什么…」专节、无决策过程叙事。\n";
report += "- 范围外能力表述为「不建设/不在范围内」。\n";
report += "- 规则可直接指导实现与验收。\n";

let report_path = write_scratch_file("report.md", report);
log("report written: " + report_path);

complete(#{
    summary: "Normalized " + files.len().to_string() + " docs; rewrite_ok=" + rewrite_ok.len().to_string() + " fail=" + rewrite_fail.len().to_string() + " fix=" + fix_paths.len().to_string(),
    path: report_path,
    file_count: files.len(),
    rewrite_ok: rewrite_ok.len(),
    rewrite_fail: rewrite_fail.len(),
    fix_count: fix_paths.len(),
});
