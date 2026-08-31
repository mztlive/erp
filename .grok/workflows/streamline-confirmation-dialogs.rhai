let meta = #{
    name: "streamline-confirmation-dialogs",
    description: "Scan ERP client submit and confirmation dialogs, correct misleading copy, simplify wording, and verify focused changes",
    when_to_use: "Use when erp-client confirmation dialogs need a focused copy audit and safe wording cleanup.",
    phases: [
        #{ title: "Scan", detail: "parallel copy and semantics audit" },
        #{ title: "Synthesize", detail: "deduplicate and verify findings" },
        #{ title: "Implement", detail: "edit only confirmed dialog copy" },
        #{ title: "Review", detail: "independent correctness and scope review" },
        #{ title: "Verify", detail: "run focused frontend checks" },
    ],
};

let root = if args == () { () } else { args.root };
if root == () { pause("verification", "Pass args.root for the frontend directory."); }

let base = "Work only under " + root + ". The workspace may contain pre-existing uncommitted changes. Preserve them exactly unless a wording edit is required for this task. Use grep and read_file to inspect actual source and tests. Focus on dialogs, alert dialogs, modals, confirmation sheets, and inline confirmation prompts that ask users to submit, approve, reject, cancel, delete, close, publish, archive, execute, or otherwise commit an action. Evaluate user-visible title, description, field labels, validation messages, confirm/cancel labels, consequences, and irreversible-action warnings. Ignore non-confirmation informational dialogs and purely technical test fixtures unless they assert production copy. ";

phase("Scan");
let scan_jobs = [];
let p1 = base;
p1 += "Audit wording quality. Find copy that is repetitive, overlong, indirect, jargon-heavy, duplicated between title and description, or padded with low-value instructions. For every concrete issue, report file path, exact current text, why it is hard to understand, and a concise replacement. A clean result is valid only after repository-wide searches for dialog primitives and confirmation-related Chinese and English terms.";
scan_jobs.push(#{ prompt: p1, label: "scan:clarity", capability_mode: "read-only" });
let p2 = base;
p2 += "Audit semantic correctness. Cross-check each confirmation message against the surrounding mutation, handler, API call, state transition, and button action. Find wording that claims the wrong operation, wrong lifecycle result, wrong reversibility, wrong scope, wrong object, or wrong success/failure consequence. Report evidence from code and an accurate replacement. Do not propose stylistic edits without reading the action implementation.";
scan_jobs.push(#{ prompt: p2, label: "scan:semantics", capability_mode: "read-only" });
let p3 = base;
p3 += "Audit consistency and risk communication. Compare confirmation dialogs across features and shared components. Find inconsistent verbs, vague generic titles such as '确认操作', misleading cancel labels, missing irreversible/destructive warnings, and descriptions that expose implementation details rather than business outcomes. Report only actionable findings with file path, exact text, evidence, and replacement.";
scan_jobs.push(#{ prompt: p3, label: "scan:consistency", capability_mode: "read-only" });
let scans = parallel(scan_jobs);

let scan_text = "";
let scan_index = 0;
for s in scans {
    if s != () && s.success && s.output != () {
        scan_text += "\n\n=== SCAN " + scan_index.to_string() + " ===\n";
        scan_text += s.output;
    }
    scan_index += 1;
}
if scan_text == "" { pause("infra", "All dialog-copy scanners failed; inspect the workflow run and retry."); }

phase("Synthesize");
let synth_prompt = base;
synth_prompt += "Independently verify and deduplicate the following scanner reports by reopening every cited production file and reading the action path. Reject false positives, out-of-scope items, and changes that would weaken required warnings. Produce a prioritized implementation brief. For each accepted edit include: file, exact old text, exact replacement, classification (verbose, misleading, inconsistent, or missing consequence), and code evidence. Prefer short business language, explicit action verbs, and one clear consequence sentence. Preserve necessary entity names, amounts, statuses, and irreversible-action warnings. Scanner reports follow: ";
synth_prompt += json_encode(scan_text);
let synth = agent(synth_prompt, #{ label: "synthesize:verified-findings", capability_mode: "read-only" });
if synth == () || !synth.success || synth.output == () { pause("verification", "The synthesis agent did not produce a verified implementation brief."); }

phase("Implement");
let impl_prompt = base;
impl_prompt += "Apply the verified implementation brief below. Reopen every target file before editing. Change only user-visible confirmation wording and directly related copy assertions in tests. Preserve component behavior, IDs, event handlers, API payloads, validation logic, and unrelated pre-existing edits. Keep wording concise, direct, accurate to the actual action, and consistent with nearby product language. If a proposed edit is contradicted by code, skip it and explain why. Add or update focused tests only when production copy is already covered or the semantic distinction needs regression protection. After editing, run formatting only on files you changed if supported. Verified brief: ";
impl_prompt += json_encode(synth.output);
let impl = agent(impl_prompt, #{ label: "implement:dialog-copy", capability_mode: "read-write" });
if impl == () || !impl.success { pause("blocked", "The implementation agent failed; inspect its output before retrying."); }

phase("Review");
let review_jobs = [];
let r1 = base;
r1 += "Review the current workspace changes for this task after the implementation agent. Inspect git diff and relevant source. Verify every changed confirmation sentence accurately describes the handler or mutation it triggers, including scope, lifecycle transition, reversibility, and destructive consequences. Report concrete problems only; say PASS only after reading the changed code paths.";
review_jobs.push(#{ prompt: r1, label: "review:semantic-accuracy", capability_mode: "read-only" });
let r2 = base;
r2 += "Review the current workspace changes for this task for copy quality and scope. Check that titles and descriptions are concise and non-repetitive, button labels are explicit, necessary warnings remain, tests match production copy, no unrelated code was changed, and pre-existing edits were preserved. Report concrete file-level issues only; say PASS only after inspecting the diff.";
review_jobs.push(#{ prompt: r2, label: "review:clarity-and-scope", capability_mode: "read-only" });
let reviews = parallel(review_jobs);

let review_text = "";
for r in reviews {
    if r != () && r.success && r.output != () {
        review_text += "\n\n";
        review_text += r.output;
    }
}

let fix_prompt = base;
fix_prompt += "Resolve every concrete issue in the independent review reports below. Verify each claim against source before editing. Keep fixes limited to confirmation copy and directly related tests, preserve unrelated workspace changes, and skip unsupported reviewer suggestions. Review reports: ";
fix_prompt += json_encode(review_text);
let fix = agent(fix_prompt, #{ label: "implement:review-fixes", capability_mode: "read-write" });
if fix == () || !fix.success { pause("blocked", "The review-fix agent failed; inspect its output before retrying."); }

phase("Verify");
let verify_prompt = "Work in the repository containing " + root + ". Inspect the final diff and project scripts. Run the narrowest relevant frontend tests for every changed dialog/test file, then run the appropriate TypeScript typecheck and lint commands if they are available and reasonably scoped. Do not edit source merely to hide failures. If a failure is caused by this task, fix it and rerun the failing check; preserve unrelated pre-existing changes. Report commands, pass/fail status, changed files, and any checks that could not run. Also state that interactive browser verification was not performed unless you actually launched the app and exercised the affected dialogs end to end.";
let verify = agent(verify_prompt, #{ label: "verify:frontend-gates", capability_mode: "all" });
if verify == () || !verify.success || verify.output == () { pause("verification", "Frontend verification did not complete successfully."); }

let report = "# Confirmation dialog copy workflow\n\n## Verified findings\n";
report += synth.output;
report += "\n\n## Implementation\n";
if impl.output != () { report += impl.output; }
report += "\n\n## Independent reviews\n";
report += review_text;
report += "\n\n## Review fixes\n";
if fix.output != () { report += fix.output; }
report += "\n\n## Verification\n";
report += verify.output;
let report_path = write_scratch_file("confirmation-dialog-copy-report.md", report);
complete(#{ summary: "Scanned, edited, reviewed, and verified confirmation dialog copy under the requested frontend root.", report: report, path: report_path });
