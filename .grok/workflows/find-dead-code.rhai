let meta = #{
    name: "find-dead-code",
    description: "扫描仓库死代码并经对抗验证后输出报告",
    when_to_use: "当需要分析项目中未使用的函数、组件、导出或文件时",
    phases: [
        #{ title: "Scan", detail: "分区扫描可疑死代码" },
        #{ title: "Verify", detail: "逐条对抗验证" },
        #{ title: "Report", detail: "汇总死代码报告" },
    ],
};

let scan_schema = #{
    "type": "object",
    "required": ["findings"],
    "properties": #{
        "findings": #{
            "type": "array", "maxItems": 20,
            "items": #{
                "type": "object",
                "required": ["file", "symbol", "kind", "reason"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "symbol": #{ "type": "string" },
                    "kind": #{ "type": "string" },
                    "reason": #{ "type": "string" },
                },
            },
        },
    },
};

let verdict_schema = #{
    "type": "object",
    "required": ["dead", "reason", "evidence"],
    "properties": #{
        "dead": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "evidence": #{ "type": "string" },
    },
};

let report_schema = #{
    "type": "object",
    "required": ["summary", "report_markdown"],
    "properties": #{
        "summary": #{ "type": "string" },
        "report_markdown": #{ "type": "string" },
    },
};

fn has_prefix(path, prefix) {
    if type_of(path) != "string" { return false; }
    if type_of(prefix) != "string" { return false; }
    if prefix == "." { return true; }
    if prefix == "" { return true; }
    if path.len() < prefix.len() { return false; }
    return path.sub_string(0, prefix.len()) == prefix;
}

let root = ".";
let focus = "all";
if type_of(args) == "map" {
    if args.root != () { root = args.root; }
    if args.focus != () { focus = args.focus; }
}

phase("Scan");
let jobs = [];
if focus == "all" || focus == "backend" {
    let p1 = "你是 Rust 死代码扫描器。仓库根为 " + root + "。";
    p1 += "只报告该根下的文件，其他路径一律忽略。";
    p1 += "使用 grep 在仓库内搜索定义与引用，用 read_file 精读候选文件，不要靠记忆回答。";
    p1 += "重点找：从未被调用的 pub 函数与方法、从未被构造的 struct 与 enum、从未被引用的模块与常量。";
    p1 += "排除：测试代码、入口 main、框架回调、仅用于序列化的字段。";
    p1 += "每条给出 file、symbol、kind、reason，最多 20 条；精读代码后确认无嫌疑则返回空数组。";
    jobs.push(#{ prompt: p1, label: "scan:rust", capability_mode: "read-only", output_schema: scan_schema });
}
if focus == "all" || focus == "frontend" {
    let p2 = "你是前端死代码扫描器。仓库根为 " + root + "。";
    p2 += "只报告该根下的文件，其他路径一律忽略。";
    p2 += "使用 grep 搜索导出与导入关系，用 read_file 精读候选文件，不要靠记忆回答。";
    p2 += "重点找：从未被导入的组件与 hooks、从未被调用的工具函数、从未被访问的路由与页面。";
    p2 += "排除：测试与 story 文件、环境类型声明、仅供外部插件调用的导出。";
    p2 += "每条给出 file、symbol、kind、reason，最多 20 条；精读后无嫌疑则返回空数组。";
    jobs.push(#{ prompt: p2, label: "scan:frontend", capability_mode: "read-only", output_schema: scan_schema });
}
if focus == "all" {
    let p3 = "你是脚本与配置死代码扫描器。仓库根为 " + root + "。";
    p3 += "只报告该根下的文件，其他路径一律忽略。";
    p3 += "使用 grep 搜索脚本名与配置键的引用，用 read_file 精读 package.json、CI 配置与 scripts 目录，不要靠记忆回答。";
    p3 += "重点找：从未被调用的脚本、从未被读取的配置键、已废弃的依赖。";
    p3 += "每条给出 file、symbol、kind、reason，最多 20 条；精读后无嫌疑则返回空数组。";
    jobs.push(#{ prompt: p3, label: "scan:scripts", capability_mode: "read-only", output_schema: scan_schema });
}
let results = parallel(jobs);
let findings = [];
for r in results {
    if r != () && r.success && r.output != () && r.output.findings != () {
        for f in r.output.findings { findings.push(f); }
    }
}
let kept = [];
let dropped = 0;
for f in findings {
    if f != () && f.file != () && has_prefix(f.file, root) {
        kept.push(f);
    } else {
        dropped += 1;
    }
}
if dropped > 0 {
    log(dropped.to_string() + " out-of-scope findings dropped");
}
findings = kept;
log("scan collected " + findings.len().to_string() + " findings");
if findings.len() == 0 {
    let empty_report = "# 死代码分析报告\n\n扫描范围：" + root + "\n\n结论：未发现可疑死代码。\n";
    let empty_path = write_scratch_file("dead-code-report.md", empty_report);
    complete(#{ summary: "未发现可疑死代码", confirmed: [], path: empty_path, report: empty_report });
}
phase("Verify");
let total = findings.len();
let max_v = 24;
let n = if total < max_v { total } else { max_v };
if total > max_v {
    log(total.to_string() + " findings capped to " + max_v.to_string() + " for verification");
}
let vjobs = [];
for i in 0..n {
    let f = findings[i];
    let fq = json_encode(f);
    let pv = "你是死代码质疑者。用 grep 与 read_file 独立核查这条嫌疑：" + fq + "。";
    pv += "只有在全仓搜索确认零引用、且排除动态调用、反射、路由约定与外部入口后，才设 dead=true。";
    pv += "否则 dead=false。reason 说明结论，evidence 给出你亲自读过的引用搜索结果或文件行。";
    vjobs.push(#{ prompt: pv, label: "verify", capability_mode: "read-only", output_schema: verdict_schema });
}
let verdicts = parallel(vjobs);
let confirmed = [];
for i in 0..n {
    let v = verdicts[i];
    let f = findings[i];
    if v != () && v.success && v.output != () && v.output.dead == true {
        if v.output.evidence != () && v.output.evidence != "" {
            let item = #{
                file: f.file, symbol: f.symbol, kind: f.kind,
                reason: v.output.reason, evidence: v.output.evidence,
            };
            confirmed.push(item);
        }
    }
}
log(confirmed.len().to_string() + "/" + n.to_string() + " findings confirmed dead");
phase("Report");
let payload = json_encode(confirmed);
let pr = "你是死代码报告撰写者。扫描范围 " + root + "，已确认死代码 JSON：" + payload + "。";
pr += "用中文写一份 Markdown 报告：标题、范围与方法、按文件分组的死代码表（含 file、symbol、kind、删除建议）、高风险误删提醒、后续清理步骤。";
pr += "无确认项则明确写出结论与抽查覆盖面。返回 summary 一句话与 report_markdown 全文。";
let s = agent(pr, #{ label: "report", capability_mode: "read-only", output_schema: report_schema });
let report_text = "# 死代码分析报告\n\n扫描范围：" + root + "\n\n确认 " + confirmed.len().to_string() + " 项。\n";
let summary_text = confirmed.len().to_string() + " confirmed dead-code items in " + root;
if s != () && s.success && s.output != () && s.output.report_markdown != () {
    report_text = s.output.report_markdown;
    if s.output.summary != () { summary_text = s.output.summary; }
}
let out_path = write_scratch_file("dead-code-report.md", report_text);
complete(#{ summary: summary_text, confirmed: confirmed, path: out_path, report: report_text });
