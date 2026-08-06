// P4 前端集成：把 erp-client features 的 mock 实现换成真实 HTTP。
//
// 依据：docs/dev-plan/P4-frontend.md
// 前置：P3 全部 C-G* 已合并；F7 额外依赖 E3（投影），缺失时登记缺口不造数据。
//
// Run: /workflow p4-frontend-integrate
// Args (all optional):
//   batches: "all" | "F1,F2,..."  (default "all")
//   skip_verify: bool            (default false)
//   skip_merge: bool             (default true — 并行 worktree 完成后只出报告，不自动合入 main)
//   base_ref: string             (default "main" — 每个 F 批次从该 ref 开分支)

let meta = #{
    name: "p4-frontend-integrate",
    description: "P4: rewrite erp-client feature api.ts from mock to real HTTP per F1-F10 batches",
    when_to_use: "When landing P4 frontend integration against merged P3 APIs",
    phases: [
        #{ title: "Select", detail: "Resolve batch list and ownership" },
        #{ title: "Integrate", detail: "Parallel worktree per F batch: contract diff + real api + delete mocks" },
        #{ title: "Verify", detail: "Adversarial check mock refs, export signatures, frozen files" },
        #{ title: "Report", detail: "Write evidence summary under docs/dev-plan/p4-evidence" },
    ],
};

// --- args ---
let batches_arg = "all";
let skip_verify = false;
let skip_merge = true;
let base_ref = "main";
if args != () {
    if args.batches != () { batches_arg = args.batches; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
    if args.skip_merge != () { skip_merge = args.skip_merge; }
    if args.base_ref != () { base_ref = args.base_ref; }
}

// F1–F10 fixed catalog from P4-frontend.md / _meta.json
let all_batches = [
    #{
        id: "F1",
        branch: "feat/erp-f1-master-data",
        features: ["master-data"],
        workspaces: ["W14"],
        app_routes: ["erp-client/app/(workspace)/master-data"],
        backend_domains: ["catalog", "warehouse", "supplier", "party"],
        depends_on: ["C-G2", "C-G3"],
        notes: "W14 基础资料：可售商品/产品/类目/品牌/卡券类目/供应商/仓库；queries 内联 mock 需抽出 api.ts",
    },
    #{
        id: "F2",
        branch: "feat/erp-f2-customer-contract",
        features: ["customers", "contracts"],
        workspaces: ["W03", "W04"],
        app_routes: [
            "erp-client/app/(workspace)/sales/customers",
            "erp-client/app/(workspace)/sales/contracts",
        ],
        backend_domains: ["customer", "party", "contract"],
        depends_on: ["C-G2", "C-G4"],
        notes: "客户中心 + 合同；customers 的 fetch 在 queries.ts/session，需迁到 api.ts",
    },
    #{
        id: "F3",
        branch: "feat/erp-f3-sales-orders",
        features: ["sales-orders"],
        workspaces: ["W05", "W06"],
        app_routes: ["erp-client/app/(workspace)/sales/orders"],
        backend_domains: ["sales_order", "sales_review", "fulfillment"],
        depends_on: ["C-G4", "C-G6"],
        notes: "工作量最大之一：验收工作台、变更单、卡券审批、采购拒绝处理",
    },
    #{
        id: "F4",
        branch: "feat/erp-f4-procurement",
        features: ["procurement-confirmation", "purchase-orders"],
        workspaces: ["W07", "W08"],
        app_routes: [
            "erp-client/app/(workspace)/procurement/confirm",
            "erp-client/app/(workspace)/procurement/orders",
        ],
        backend_domains: ["purchase_order", "sales_review", "supplier"],
        depends_on: ["C-G4", "C-G5"],
        notes: "采购确认队列 + 采购单",
    },
    #{
        id: "F5",
        branch: "feat/erp-f5-fulfillment",
        features: ["fulfillment-operations", "inventory"],
        workspaces: ["W09", "W10"],
        app_routes: [
            "erp-client/app/(workspace)/fulfillment",
            "erp-client/app/(workspace)/inventory",
        ],
        backend_domains: ["fulfillment", "inventory", "work_item"],
        depends_on: ["C-G6"],
        notes: "工作量最大之一：履约作业 17 文件 + 库存台账",
    },
    #{
        id: "F6",
        branch: "feat/erp-f6-finance",
        features: ["customer-receivables", "supplier-payables", "card-funds-review"],
        workspaces: ["W11", "W12", "W13"],
        app_routes: [
            "erp-client/app/(workspace)/finance/customer-accounts",
            "erp-client/app/(workspace)/finance/supplier-accounts",
            "erp-client/app/(workspace)/finance/card-funds-review",
        ],
        backend_domains: ["receivable", "payable", "card_instance", "cost"],
        depends_on: ["C-G7"],
        notes: "应收/应付/卡券资金复核",
    },
    #{
        id: "F7",
        branch: "feat/erp-f7-workbench-analytics",
        features: [
            "workspace",
            "unified-task-queue",
            "customer-quality",
            "actual-profit-loss",
            "card-business-analytics",
        ],
        workspaces: ["W01", "W02", "W15", "W16", "W28"],
        app_routes: [
            "erp-client/app/(workspace)/workspace",
            "erp-client/app/(workspace)/analytics",
        ],
        backend_domains: ["work_item", "projection", "receivable", "card_instance"],
        depends_on: ["C-G1", "C-G7", "C-G10", "E3"],
        notes: "依赖 E3 异步投影；缺 as_of/汇总接口时登记后端缺口，禁止前端造数据或客户端时间冒充 as_of",
    },
    #{
        id: "F8",
        branch: "feat/erp-f8-sync-import-audit",
        features: ["mall-sync", "import-opening", "access-audit"],
        workspaces: ["W17", "W18", "W19"],
        app_routes: [
            "erp-client/app/(workspace)/governance/mall-sync",
            "erp-client/app/(workspace)/governance/imports",
            "erp-client/app/(workspace)/system/access-audit",
        ],
        backend_domains: ["mall_sync", "legacy_import", "access_control", "source_registry"],
        depends_on: ["C-G1", "C-G8"],
        notes: "mall-sync 已有 fetchSourceSystems 样板（apiGet /admin/source-systems）；扩到全 feature",
    },
    #{
        id: "F9",
        branch: "feat/erp-f9-supply-publication",
        features: [
            "supplier-api-connections",
            "supplier-catalog",
            "product-publications",
            "execution-projections",
        ],
        workspaces: ["W20", "W21", "W22", "W23"],
        app_routes: [
            "erp-client/app/(workspace)/supplier-api/connections",
            "erp-client/app/(workspace)/procurement/supplier-catalog",
            "erp-client/app/(workspace)/commerce/publications",
            "erp-client/app/(workspace)/commerce/execution-projections",
        ],
        backend_domains: ["supplier_api", "supplier_catalog", "publication", "projection"],
        depends_on: ["C-G5", "C-G9"],
        notes: "供应商 API 连接、供应商商品库、上架发布、执行投影",
    },
    #{
        id: "F10",
        branch: "feat/erp-f10-mall-supplier-ops",
        features: [
            "mall-consumption-orders",
            "supplier-orders",
            "supplier-settlements",
            "integration-errors",
            "history-backfill",
        ],
        workspaces: ["W25", "W26", "W27", "W29", "W30"],
        app_routes: [
            "erp-client/app/(workspace)/commerce/consumption-orders",
            "erp-client/app/(workspace)/supplier-api/orders",
            "erp-client/app/(workspace)/supplier-api/settlements",
            "erp-client/app/(workspace)/governance/integration-errors",
            "erp-client/app/(workspace)/governance/history-backfill",
        ],
        backend_domains: [
            "mall_order",
            "mall_after_sales",
            "supplier_fulfillment",
            "supplier_settlement",
            "integration_ops",
            "mall_backfill",
        ],
        depends_on: ["C-G10", "C-G11", "C-G12"],
        notes: "商城消费单、供应商订单、结算、集成差错、历史回填",
    },
];

fn batch_selected(batches_arg, batch_id) {
    if batches_arg == "all" || batches_arg == () || batches_arg == "" {
        return true;
    }
    // comma-separated match
    let parts = batches_arg.split(",");
    for p in parts {
        p.trim();
        if p == batch_id { return true; }
    }
    false
}

phase("Select");
let selected = [];
for b in all_batches {
    if batch_selected(batches_arg, b.id) {
        selected.push(b);
    }
}
if selected.len() == 0 {
    pause("verification", "No batches selected. Pass args.batches as all or F1,F2,...");
}
log("P4 integrate batches=" + selected.len().to_string() + " base_ref=" + base_ref + " skip_merge=" + skip_merge.to_string());

let result_schema = #{
    "type": "object",
    "required": [
        "batch_id",
        "branch",
        "success",
        "features_done",
        "mock_files_deleted",
        "contract_gaps",
        "files_changed",
        "evidence_path",
        "summary",
        "blockers"
    ],
    "properties": #{
        "batch_id": #{ "type": "string" },
        "branch": #{ "type": "string" },
        "success": #{ "type": "boolean" },
        "features_done": #{
            "type": "array",
            "maxItems": 12,
            "items": #{ "type": "string" },
        },
        "mock_files_deleted": #{
            "type": "array",
            "maxItems": 40,
            "items": #{ "type": "string" },
        },
        "contract_gaps": #{
            "type": "array",
            "maxItems": 40,
            "items": #{
                "type": "object",
                "required": ["feature", "field_or_endpoint", "handling"],
                "properties": #{
                    "feature": #{ "type": "string" },
                    "field_or_endpoint": #{ "type": "string" },
                    "handling": #{ "type": "string" },
                },
            },
        },
        "files_changed": #{
            "type": "array",
            "maxItems": 80,
            "items": #{ "type": "string" },
        },
        "evidence_path": #{ "type": "string" },
        "summary": #{ "type": "string" },
        "blockers": #{
            "type": "array",
            "maxItems": 20,
            "items": #{ "type": "string" },
        },
        "worktree_hint": #{ "type": "string" },
        "lint_ok": #{ "type": "boolean" },
        "tsc_ok": #{ "type": "boolean" },
        "mock_grep_clean": #{ "type": "boolean" },
    },
};

let verdict_schema = #{
    "type": "object",
    "required": ["batch_id", "pass", "issues", "evidence"],
    "properties": #{
        "batch_id": #{ "type": "string" },
        "pass": #{ "type": "boolean" },
        "issues": #{
            "type": "array",
            "maxItems": 20,
            "items": #{ "type": "string" },
        },
        "evidence": #{ "type": "string" },
    },
};

// --- build integrate prompts ---
phase("Integrate");
let jobs = [];
for b in selected {
    let features_csv = "";
    let fi = 0;
    for f in b.features {
        if fi > 0 { features_csv += ", "; }
        features_csv += f;
        fi += 1;
    }
    let routes_csv = "";
    let ri = 0;
    for r in b.app_routes {
        if ri > 0 { routes_csv += ", "; }
        routes_csv += r;
        ri += 1;
    }
    let domains_csv = "";
    let di = 0;
    for d in b.backend_domains {
        if di > 0 { domains_csv += ", "; }
        domains_csv += d;
        di += 1;
    }
    let deps_csv = "";
    let ei = 0;
    for d in b.depends_on {
        if ei > 0 { deps_csv += ", "; }
        deps_csv += d;
        ei += 1;
    }

    let prompt = "";
    prompt += "You are implementing P4 frontend integration for batch " + b.id + ".\n";
    prompt += "Work ONLY in an isolated git worktree on branch " + b.branch + ".\n";
    prompt += "Repo root is the worktree cwd. Base ref for branching: " + base_ref + ".\n\n";

    prompt += "## Batch card\n";
    prompt += "- id: " + b.id + "\n";
    prompt += "- branch: " + b.branch + "\n";
    prompt += "- features: " + features_csv + "\n";
    prompt += "- workspaces: " + json_encode(b.workspaces) + "\n";
    prompt += "- app_routes owns: " + routes_csv + "\n";
    prompt += "- backend domains to map: " + domains_csv + "\n";
    prompt += "- depends_on: " + deps_csv + "\n";
    prompt += "- notes: " + b.notes + "\n\n";

    prompt += "## Authority (read before coding)\n";
    prompt += "1. docs/dev-plan/P4-frontend.md (this stage contract)\n";
    prompt += "2. docs/dev-plan/api-contract.md (envelope, paging, errors, money as strings)\n";
    prompt += "3. docs/dev-plan/conventions.md §1 owns, §2 frozen files\n";
    prompt += "4. erp-client/AGENTS.md (TanStack Query only, no bare fetch in components, ApiError)\n";
    prompt += "5. docs/ui-glossary.md + docs/ui-workspaces/wNN.md for each workspace\n";
    prompt += "6. docs/erp-data-model.md when field names conflict\n";
    prompt += "7. Reference sample: erp-client/features/mall-sync/api.ts fetchSourceSystems uses apiGet from @/lib/api\n\n";

    prompt += "## Frozen files — DO NOT MODIFY\n";
    prompt += "erp-client/lib/api/**, erp-client/lib/query-client.ts, and all backend/** unless absolutely required.\n";
    prompt += "Do NOT edit feature-source.ts REAL_FEATURES (frozen). After rewrite, api layer must call real HTTP directly;\n";
    prompt += "remove isFeatureReal mock branches inside your features.\n\n";

    prompt += "## Owns — only touch\n";
    prompt += "- erp-client/features/<feature>/** for each feature in this batch\n";
    prompt += "- listed app_routes directories if needed (prefer zero page changes)\n";
    prompt += "- erp-client/mock/<owned mock files>.ts that are exclusive to this batch features\n";
    prompt += "- docs/dev-plan/p4-evidence/" + b.id + ".md (create evidence)\n";
    prompt += "Do NOT delete erp-client/mock/session-state.ts unless you prove no other feature imports it after your changes;\n";
    prompt += "prefer leaving shared session-state and only deleting feature-owned seed mocks.\n\n";

    prompt += "## Integration steps (mandatory, use tools — do not invent from memory)\n";
    prompt += "A. git: fetch if needed; create/checkout branch " + b.branch + " from " + base_ref + ".\n";
    prompt += "B. For each feature:\n";
    prompt += "   1. Read types.ts and api.ts (or queries.ts/session.ts if api.ts missing).\n";
    prompt += "   2. Extract exported function signatures + return types (the client contract).\n";
    prompt += "   3. Read matching backend routes under backend/apps/web-api/src/core/routes/<domain>.rs\n";
    prompt += "      and DTOs under backend/services/src/<domain>/dto.rs; list HTTP paths under /admin/...\n";
    prompt += "   4. Produce a contract diff table: mock field → backend field → handling\n";
    prompt += "      (frontend_adapt | backend_gap | already_aligned).\n";
    prompt += "   5. Rewrite implementations to use apiGet/apiPost/apiPut/apiDelete from @/lib/api.\n";
    prompt += "      KEEP export signatures stable so queries.ts/pages need no change.\n";
    prompt += "      Map backend Page{items,total,page,page_size} to feature view types inside api.ts only.\n";
    prompt += "      Delete local filter/sort/page/aggregate that the server already does.\n";
    prompt += "      Keep pure display helpers; do not recompute money/qty (only format).\n";
    prompt += "      Errors: let ApiError propagate; never throw new Error(\"string\").\n";
    prompt += "   6. If feature had no api.ts (logic in queries.ts), create api.ts with real HTTP and\n";
    prompt += "      thin queries.ts to call api functions (queryKey/hooks unchanged in shape).\n";
    prompt += "   7. Remove mockDelay, @/mock/* imports, session mock modules for this feature.\n";
    prompt += "   8. Delete owned mock seed files under erp-client/mock/ when unreferenced by this batch.\n";
    prompt += "C. Hard requirements from P4:\n";
    prompt += "   - All requests via TanStack Query only (api functions only used as queryFn/mutationFn).\n";
    prompt += "   - No business fetch in Server Components.\n";
    prompt += "   - Money/qty: backend rounded strings; frontend format only.\n";
    prompt += "   - Analytics pages: show as_of from API, never client clock.\n";
    prompt += "   - Permissions: prefer lib/permissions.generated.ts; do not add demo-roles usage.\n";
    prompt += "   - Backend missing fields: register gap in evidence; do NOT invent data.\n";
    prompt += "D. Gates (run from erp-client if node_modules present):\n";
    prompt += "   - grep -r \"@/mock\" features/<feature> and app routes for this batch → expect empty\n";
    prompt += "   - npx tsc --noEmit (or note failure blockers)\n";
    prompt += "   - npm run lint if feasible\n";
    prompt += "E. Write docs/dev-plan/p4-evidence/" + b.id + ".md with:\n";
    prompt += "   - contract diff table\n";
    prompt += "   - deleted mock file list\n";
    prompt += "   - commands run + results\n";
    prompt += "   - remaining blockers / backend gaps\n";
    prompt += "F. git add owned paths + evidence; commit with message like:\n";
    prompt += "   'P4 " + b.id + ": integrate " + features_csv + " to real HTTP'\n";
    prompt += "   Do NOT push unless explicitly asked.\n\n";

    prompt += "## Success criteria for success=true\n";
    prompt += "- Every feature in the batch no longer imports @/mock for its runtime data path\n";
    prompt += "- Exported API function names used by queries.ts still exist\n";
    prompt += "- Evidence markdown written\n";
    prompt += "- Commit created on " + b.branch + "\n";
    prompt += "If blocked by missing E3 endpoints (F7) or backend gaps, set success=false only when\n";
    prompt += "you could not integrate the majority; partial with documented gaps can be success=true\n";
    prompt += "if mock paths are gone for implemented surfaces and gaps listed.\n\n";

    prompt += "Return the structured result schema. files_changed must be real paths you edited.\n";

    jobs.push(#{
        prompt: prompt,
        label: "integrate:" + b.id,
        capability_mode: "all",
        isolation_worktree: true,
        output_schema: result_schema,
    });
}

log("Launching " + jobs.len().to_string() + " integrate agents (worktree isolated)");
let integrate_results = parallel(jobs);

let batch_results = [];
let i = 0;
for r in integrate_results {
    let batch = selected[i];
    if r == () || r.success != true || r.output == () {
        log("Integrate FAILED or empty for " + batch.id);
        batch_results.push(#{
            batch_id: batch.id,
            branch: batch.branch,
            success: false,
            features_done: [],
            mock_files_deleted: [],
            contract_gaps: [],
            files_changed: [],
            evidence_path: "",
            summary: "agent failed or returned no structured output",
            blockers: ["integrate agent failed"],
            worktree_hint: "",
            lint_ok: false,
            tsc_ok: false,
            mock_grep_clean: false,
        });
    } else {
        let out = r.output;
        // force batch_id/branch identity from catalog
        out.batch_id = batch.id;
        out.branch = batch.branch;
        if out.success == () { out.success = false; }
        if out.summary == () { out.summary = ""; }
        if out.features_done == () { out.features_done = []; }
        if out.mock_files_deleted == () { out.mock_files_deleted = []; }
        if out.contract_gaps == () { out.contract_gaps = []; }
        if out.files_changed == () { out.files_changed = []; }
        if out.evidence_path == () { out.evidence_path = ""; }
        if out.blockers == () { out.blockers = []; }
        batch_results.push(out);
        if out.success == true {
            log(batch.id + " integrate ok: " + out.summary);
        } else {
            log(batch.id + " integrate incomplete: " + out.summary);
        }
    }
    i += 1;
}

// --- verify ---
let verdicts = [];
if !skip_verify {
    phase("Verify");
    let vjobs = [];
    let vi = 0;
    for br in batch_results {
        let batch = selected[vi];
        let features_csv = "";
        let fi = 0;
        for f in batch.features {
            if fi > 0 { features_csv += ", "; }
            features_csv += f;
            fi += 1;
        }
        let br_json = json_encode(br);
        let vprompt = "";
        vprompt += "Adversarially verify P4 batch " + batch.id + " integration claim.\n";
        vprompt += "Claim JSON: " + br_json + "\n\n";
        vprompt += "You are a skeptic. Use read-only tools on the CURRENT workspace and, if the claim\n";
        vprompt += "mentions a branch " + batch.branch + ", run `git show " + batch.branch + ":path` or\n";
        vprompt += "`git log " + batch.branch + " -1 --stat` / `git diff " + base_ref + "..." + batch.branch + " --stat`\n";
        vprompt += "to inspect the branch. Do NOT trust the claim without evidence.\n\n";
        vprompt += "Check:\n";
        vprompt += "1. Branch exists and has commits beyond " + base_ref + " for features: " + features_csv + "\n";
        vprompt += "2. No remaining `@/mock` imports under those features in the branch tree\n";
        vprompt += "3. api layer uses @/lib/api (apiGet/apiPost/...) not mockDelay-only paths\n";
        vprompt += "4. Frozen lib/api/** not modified on the branch\n";
        vprompt += "5. queries.ts export hook names still present\n";
        vprompt += "6. Evidence file docs/dev-plan/p4-evidence/" + batch.id + ".md exists on branch\n";
        vprompt += "7. No invented money math left that should be server-side (spot-check)\n\n";
        vprompt += "Set pass=true ONLY with concrete evidence in `evidence` (commands + short quotes).\n";
        vprompt += "If branch missing or mock remains, pass=false and list issues.\n";
        vjobs.push(#{
            prompt: vprompt,
            label: "verify:" + batch.id,
            capability_mode: "read-only",
            output_schema: verdict_schema,
        });
        vi += 1;
    }
    let vresults = parallel(vjobs);
    let vj = 0;
    for vr in vresults {
        let batch = selected[vj];
        if vr != () && vr.success == true && vr.output != () {
            let vo = vr.output;
            vo.batch_id = batch.id;
            if vo.pass == () { vo.pass = false; }
            if vo.issues == () { vo.issues = []; }
            if vo.evidence == () { vo.evidence = ""; }
            verdicts.push(vo);
            if vo.pass == true {
                log("Verify PASS " + batch.id);
            } else {
                log("Verify FAIL " + batch.id + ": " + json_encode(vo.issues));
            }
        } else {
            verdicts.push(#{
                batch_id: batch.id,
                pass: false,
                issues: ["verifier agent failed"],
                evidence: "",
            });
            log("Verify AGENT FAIL " + batch.id);
        }
        vj += 1;
    }
} else {
    log("skip_verify=true — skipping adversarial verification");
}

// --- report ---
phase("Report");
let ok_count = 0;
let fail_count = 0;
let report = "# P4 Frontend Integration Run Report\n\n";
report += "base_ref: `" + base_ref + "`\n\n";
report += "batches_arg: `" + batches_arg + "`\n\n";
report += "| Batch | Branch | Integrate | Verify | Summary |\n";
report += "| --- | --- | --- | --- | --- |\n";

let bi = 0;
for br in batch_results {
    let integ = "FAIL";
    let br_ok = br.success == true;
    if br_ok { integ = "OK"; ok_count += 1; } else { fail_count += 1; }
    let ver = "n/a";
    if !skip_verify {
        ver = "FAIL";
        for v in verdicts {
            if v.batch_id == br.batch_id {
                if v.pass == true { ver = "PASS"; } else { ver = "FAIL"; }
            }
        }
    }
    let sum = br.summary;
    if type_of(sum) != "string" { sum = ""; }
    report += "| " + br.batch_id + " | `" + br.branch + "` | " + integ + " | " + ver + " | " + sum + " |\n";
    bi += 1;
}

report += "\n## Details\n\n";
for br in batch_results {
    let br_ok2 = br.success == true;
    let evid = br.evidence_path;
    if evid == () { evid = ""; }
    report += "### " + br.batch_id + "\n\n";
    report += "- branch: `" + br.branch + "`\n";
    report += "- success: " + br_ok2.to_string() + "\n";
    report += "- evidence: " + evid + "\n";
    report += "- features_done: " + json_encode(br.features_done) + "\n";
    report += "- mock_files_deleted: " + json_encode(br.mock_files_deleted) + "\n";
    report += "- contract_gaps: " + json_encode(br.contract_gaps) + "\n";
    report += "- blockers: " + json_encode(br.blockers) + "\n";
    report += "- files_changed count: see agent output\n\n";
}

if !skip_verify {
    report += "## Verification\n\n";
    for v in verdicts {
        let vpass = v.pass == true;
        let vevid = v.evidence;
        if vevid == () { vevid = ""; }
        report += "### " + v.batch_id + " — pass=" + vpass.to_string() + "\n\n";
        report += "- issues: " + json_encode(v.issues) + "\n";
        report += "- evidence: " + vevid + "\n\n";
    }
}

report += "## Merge instructions\n\n";
report += "Agents used `isolation_worktree`; branches live in their worktrees / local refs.\n";
report += "To land a batch after review:\n\n";
report += "```bash\ngit checkout main\ngit merge --no-ff feat/erp-fN-...\n```\n\n";
report += "Resolve conflicts carefully on shared mocks (session-state) and app shell.\n";
report += "Do not merge F7 until E3 projection endpoints exist, unless gaps are accepted.\n";
report += "skip_merge=" + skip_merge.to_string() + " (this run did not auto-merge).\n";

let report_path = write_scratch_file("p4-report.md", report);
log("Report written: " + report_path + " ok=" + ok_count.to_string() + " fail=" + fail_count.to_string());

// Optional: if skip_merge false, pause for human to confirm merge policy
if !skip_merge {
    await_user("user", "skip_merge=false but auto-merge is not implemented in-script. Merge branches listed in the report manually, then resume to finish.");
}

complete(#{
    summary: "P4 integrate done: ok=" + ok_count.to_string() + " fail=" + fail_count.to_string(),
    ok_count: ok_count,
    fail_count: fail_count,
    batches: batch_results,
    verdicts: verdicts,
    report_path: report_path,
    skip_merge: skip_merge,
});
