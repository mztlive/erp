// analyze-large-files.rhai
// 扫描 Rust 代码库中的大文件，逐个深入分析并生成结构化文件拆分方案。
//
// 用法:
//   workflow analyze-large-files
//   workflow analyze-large-files args={root:"backend", min_lines:1000, list_min_lines:800, max_files:40}
//
// 参数:
//   root           - 扫描根目录（相对仓库根），默认 "backend"
//   min_lines      - 深入分析的行数阈值，默认 1000
//   list_min_lines - 完整清单（仅列出不分析）的阈值，默认 800
//   max_files      - 深入分析的文件数量上限，默认 40
//   seed_files     - 可选兜底清单 [{path, lines}, ...]，扫描失败时使用（也用于离线冒烟）
//
// 输出:
//   path      - scratch/split-plan.md 拆分方案报告
//   report    - Markdown 报告全文
//   files     - 分析过的文件清单 [{path, lines}]
//   proposals - 结构化拆分建议（供确认后驱动执行）

let meta = #{
    name: "analyze-large-files",
    description: "扫描 Rust 代码库找出所有大文件，逐个分析并输出结构化文件拆分方案，供人工确认后执行拆分",
    when_to_use: "需要定位仓库中超过行数阈值的大文件并生成拆分方案时",
    phases: [
        #{ title: "Scan", detail: "枚举大文件清单" },
        #{ title: "Analyze", detail: "每个大文件一个分析代理" },
        #{ title: "Synthesize", detail: "汇总为拆分方案报告" },
    ],
};

let scan_schema = #{
    "type": "object",
    "required": ["files"],
    "properties": #{
        "files": #{
            "type": "array",
            "maxItems": 300,
            "items": #{
                "type": "object",
                "required": ["path", "lines"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "lines": #{ "type": "integer", "minimum": 1 },
                },
            },
        },
    },
};

let proposal_schema = #{
    "type": "object",
    "required": ["file", "verdict", "summary", "proposal", "effort", "risk"],
    "properties": #{
        "file": #{ "type": "string" },
        "verdict": #{ "type": "string", "enum": ["split", "keep"] },
        "summary": #{ "type": "string" },
        "proposal": #{
            "type": "array",
            "maxItems": 6,
            "items": #{
                "type": "object",
                "required": ["new_file", "contents"],
                "properties": #{
                    "new_file": #{ "type": "string" },
                    "contents": #{ "type": "string" },
                    "moved_from_original": #{ "type": "string" },
                    "dependency_notes": #{ "type": "string" },
                },
            },
        },
        "effort": #{ "type": "string", "enum": ["S", "M", "L", "XL"] },
        "risk": #{ "type": "string", "enum": ["low", "medium", "high"] },
    },
};

// ---------- 参数 ----------
let root = "backend";
let min_lines = 1000;
let list_min_lines = 800;
let max_files = 40;
if args != () {
    if args.root != () { root = args.root; }
    if args.min_lines != () { min_lines = args.min_lines; }
    if args.list_min_lines != () { list_min_lines = args.list_min_lines; }
    if args.max_files != () { max_files = args.max_files; }
}

// ---------- 工具函数 ----------
fn basename(p) {
    let parts = p.split("/");
    parts[parts.len() - 1]
}

fn sort_desc(files) {
    let n = files.len();
    let i = 0;
    while i < n {
        let j = 0;
        while j < n - i - 1 {
            if files[j].lines < files[j + 1].lines {
                let tmp = files[j];
                files[j] = files[j + 1];
                files[j + 1] = tmp;
            }
            j += 1;
        }
        i += 1;
    }
    files
}

fn filter_files(files, root, min_lines) {
    let out = [];
    let seen = [];
    let n = files.len();
    let i = 0;
    while i < n {
        let f = files[i];
        if type_of(f.path) == "string" && type_of(f.lines) == "i64" {
            let p = f.path;
            if p.starts_with(root + "/") && !seen.contains(p) && f.lines >= min_lines {
                seen.push(p);
                out.push(f);
            }
        }
        i += 1;
    }
    sort_desc(out)
}

fn list_rest(files, analyze_paths, root, list_min_lines) {
    let out = [];
    let n = files.len();
    let i = 0;
    while i < n {
        let f = files[i];
        if type_of(f.path) == "string" && type_of(f.lines) == "i64" {
            let p = f.path;
            if p.starts_with(root + "/") && f.lines >= list_min_lines && !analyze_paths.contains(p) {
                out.push(f);
            }
        }
        i += 1;
    }
    sort_desc(out)
}

// ---------- Phase 1: Scan ----------
phase("Scan");
let scan_prompt = "在仓库根目录执行以下命令，找出所有大文件（.rs 源文件，排除 target 目录）：\n";
scan_prompt += "find " + root + " -name '*.rs' -not -path '" + root + "/target/*' -print0 | xargs -0 wc -l | sort -rn\n";
scan_prompt += "若当前工作目录不是仓库根，先 cd 到仓库根再执行。输出中每个路径必须以 '" + root + "/' 开头（相对仓库根）。\n";
scan_prompt += "只保留行数 >= " + list_min_lines.to_string() + " 的文件，按行数降序输出为 {path, lines} 数组。\n";
scan_prompt += "路径示例: " + root + "/services/src/fulfillment/mod.rs。不要凭记忆编造，必须真实执行命令后解析结果。";

let scan_result = agent(scan_prompt, #{
    label: "scan-large-files",
    capability_mode: "execute",
    output_schema: scan_schema,
});

let scan_files = [];
if scan_result != () && scan_result.success && type_of(scan_result.output) == "map" && scan_result.output.files != () {
    scan_files = scan_result.output.files;
}
if scan_files.len() == 0 && args != () && args.seed_files != () {
    scan_files = args.seed_files;
    log("扫描结果不可用，使用 args.seed_files 作为文件清单");
}

let all_files = filter_files(scan_files, root, list_min_lines);
let analyze_list = [];
if all_files.len() > 0 {
    analyze_list = filter_files(scan_files, root, min_lines);
    let total = analyze_list.len();
    if total > max_files {
        let capped = [];
        let k = 0;
        while k < max_files {
            capped.push(analyze_list[k]);
            k += 1;
        }
        analyze_list = capped;
        log("大文件数量超过上限 " + max_files.to_string() + "，仅深入分析前 " + max_files.to_string() + " 个，其余 "
            + (total - max_files).to_string() + " 个只列入清单");
    }
}
log("找到 >= " + min_lines.to_string() + " 行的大文件 " + analyze_list.len().to_string() + " 个");

if analyze_list.len() == 0 {
    complete(#{
        path: "",
        report: "扫描未找到行数 >= " + min_lines.to_string() + " 的 .rs 文件（可用参数 min_lines / list_min_lines / max_files 调整阈值）。",
        files: [],
        proposals: [],
    });
}

// ---------- Phase 2: Analyze ----------
phase("Analyze");
let jobs = [];
let n = analyze_list.len();
let i = 0;
while i < n {
    let f = analyze_list[i];
    let p = f.path;
    let lines = f.lines;
    let prompt = "请分析后端代码库中的大文件并给出文件拆分方案（结论与摘要用中文）。\n";
    prompt += "目标文件: " + p + "，约 " + lines.to_string() + " 行。\n";
    prompt += "步骤：\n";
    prompt += "1) 先用 read_file 读取 backend/AGENTS.md，了解仓库分层与模块组织约定；\n";
    prompt += "2) 用 read_file 分块读完整目标文件（每次 offset/limit 约 900 行，直到覆盖全部 " + lines.to_string() + " 行），不要凭记忆作答；\n";
    prompt += "3) 识别内聚簇：公开类型（struct/enum/trait）、impl 块、成组函数、私有 helper；\n";
    prompt += "4) 按仓库约定设计拆分：services/* 保持 mod.rs 为模块根并 re-export，拆分时新建域内子文件（如 service.rs、xxx.rs）；";
    prompt += "database/src/repository/* 参考 purchase_order/ 子目录模式；entities/* 按聚合拆分到域目录下文件；\n";
    prompt += "5) 输出结构化方案。\n";
    prompt += "要求：new_file 必须是相对仓库根的完整路径（如 backend/services/src/xxx/yyy.rs）；contents 说明每个新文件放哪些项（写出具体项名）；";
    prompt += "moved_from_original 说明从原文件迁移了哪些函数/类型；dependency_notes 注明依赖/可见性注意点（私有 helper 迁移、super:: 引用、潜在循环依赖）。\n";
    prompt += "verdict: 若拆分成若干文件后每个文件能控制在约 800 行以内则 split，否则 keep；effort 取值 S/M/L/XL；risk 取值 low/medium/high；";
    prompt += "summary 用中文概括拆分思路与理由；proposal 最多 6 个新文件。";
    jobs.push(#{
        prompt: prompt,
        label: "analyze:" + basename(p),
        capability_mode: "read-only",
        output_schema: proposal_schema,
    });
    i += 1;
}
let results = parallel(jobs);

// ---------- Phase 3: Synthesize ----------
phase("Synthesize");
let analyze_paths = [];
let m = analyze_list.len();
let mi = 0;
while mi < m {
    analyze_paths.push(analyze_list[mi].path);
    mi += 1;
}
let rest = list_rest(scan_files, analyze_paths, root, list_min_lines);

let report = "# 后端大文件拆分方案（分析报告）\n\n";
report += "## 扫描概览\n\n";
report += "- 扫描根目录：`" + root + "`\n";
report += "- 深入分析阈值：>= " + min_lines.to_string() + " 行，共 " + analyze_list.len().to_string() + " 个文件\n";
report += "- 完整大文件清单阈值：>= " + list_min_lines.to_string() + " 行，共 " + all_files.len().to_string() + " 个文件\n\n";
report += "> 由 workflow `analyze-large-files` 自动生成；确认或调整方案后，可据此执行拆分。\n\n";
report += "## 拆分总览\n\n";
report += "| # | 文件 | 行数 | 结论 | 工作量 | 风险 |\n";
report += "|---|------|------|------|--------|------|\n";

let ri = 0;
while ri < results.len() {
    let r = results[ri];
    let f = analyze_list[ri];
    let line = "| " + (ri + 1).to_string() + " | `" + f.path + "` | " + f.lines.to_string() + " | ";
    if r != () && r.success && type_of(r.output) == "map" && r.output.verdict != () {
        line += r.output.verdict + " | " + r.output.effort + " | " + r.output.risk + " |";
    } else {
        line += "分析失败 | - | - |";
    }
    report += line + "\n";
    ri += 1;
}

report += "\n## 详细拆分方案\n\n";
let di = 0;
while di < results.len() {
    let r = results[di];
    let f = analyze_list[di];
    report += "### " + (di + 1).to_string() + ". `" + f.path + "`（" + f.lines.to_string() + " 行）\n\n";
    if r == () || !r.success || type_of(r.output) != "map"
        || r.output.verdict == () || r.output.summary == () || r.output.effort == () || r.output.risk == () {
        report += "分析代理失败，未生成方案。\n\n";
        di += 1;
        continue;
    }
    let o = r.output;
    report += "- 结论：**" + o.verdict + "**（工作量 " + o.effort + "，风险 " + o.risk + "）\n";
    report += "- 摘要：" + o.summary + "\n";
    if o.proposal != () && o.proposal.len() > 0 {
        report += "- 拆分建议：\n";
        let pj = 0;
        while pj < o.proposal.len() {
            let item = o.proposal[pj];
            if item.new_file != () && item.contents != () {
                report += "  - **" + item.new_file + "**：" + item.contents + "\n";
            }
            if item.dependency_notes != () && item.dependency_notes != "" {
                report += "    - 依赖/注意：" + item.dependency_notes + "\n";
            }
            pj += 1;
        }
    }
    report += "\n";
    di += 1;
}

if rest.len() > 0 {
    report += "## 其余大文件（>= " + list_min_lines.to_string() + " 行，本次未深入分析）\n\n";
    let k = 0;
    while k < rest.len() {
        report += "- `" + rest[k].path + "`（" + rest[k].lines.to_string() + " 行）\n";
        k += 1;
    }
    report += "\n";
}

let proposals = [];
let pi = 0;
while pi < results.len() {
    let r = results[pi];
    if r != () && r.success && type_of(r.output) == "map" && r.output.verdict != () {
        proposals.push(r.output);
    }
    pi += 1;
}
let report_path = write_scratch_file("split-plan.md", report);
log("拆分方案已生成：" + report_path + "（分析 " + analyze_list.len().to_string() + " 个文件，成功 " + proposals.len().to_string() + " 个）");
complete(#{
    path: report_path,
    report: report,
    files: analyze_list,
    proposals: proposals,
});
