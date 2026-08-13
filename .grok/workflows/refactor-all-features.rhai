let meta = #{
    name: "refactor-all-features",
    description: "Fan out one refactor agent per erp-client feature: reorganize each feature into api/hooks/components/pages/lib with single-responsibility files and re-export shims, then verify whole-repo typecheck/lint and fix regressions",
    when_to_use: "when every directory under erp-client/features/ needs the self-describing api/hooks/components/pages/lib layout without changing functional shape",
    phases: [
        #{ title: "Refactor", detail: "one agent per feature, two panels of up to 16" },
        #{ title: "Verify", detail: "whole-repo tsc + oxlint + git scope" },
        #{ title: "Fix", detail: "one fixer per feature with problems" },
        #{ title: "Final verify", detail: "re-run tsc + oxlint + git scope" },
    ],
};

let FEATURES = [
    "access-audit",
    "actual-profit-loss",
    "admin",
    "auth",
    "card-business-analytics",
    "card-funds-review",
    "contracts",
    "customer-quality",
    "customer-receivables",
    "customers",
    "entity-selectors",
    "execution-projections",
    "file-assets",
    "fulfillment-operations",
    "history-backfill",
    "import-opening",
    "integration-errors",
    "inventory",
    "mall-consumption-orders",
    "mall-sync",
    "master-data",
    "procurement-confirmation",
    "product-publications",
    "purchase-orders",
    "sales-orders",
    "supplier-api-connections",
    "supplier-offerings",
    "supplier-orders",
    "supplier-payables",
    "supplier-settlements",
    "unified-task-queue",
    "workspace",
];

if args != () {
    let only = args.only;
    if only != () && type_of(only) == "array" && only.len() > 0 {
        FEATURES = only;
    }
}

let refactor_schema = #{
    "type": "object",
    "required": ["feature", "ok", "public_entries", "moves", "notes", "errors"],
    "properties": #{
        "feature": #{ "type": "string" },
        "ok": #{ "type": "boolean" },
        "public_entries": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "moves": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "notes": #{ "type": "string" },
        "errors": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["passed", "problems", "notes"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "problems": #{
            "type": "array",
            "maxItems": 40,
            "items": #{
                "type": "object",
                "required": ["feature", "detail"],
                "properties": #{
                    "feature": #{ "type": "string" },
                    "detail": #{ "type": "string" },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

let fixer_schema = #{
    "type": "object",
    "required": ["feature", "fixed", "notes"],
    "properties": #{
        "feature": #{ "type": "string" },
        "fixed": #{ "type": "boolean" },
        "notes": #{ "type": "string" },
    },
};

let final_schema = #{
    "type": "object",
    "required": ["passed", "summary", "remaining"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "remaining": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
    },
};

phase("Refactor");
let jobs1 = [];
let jobs2 = [];
let half = FEATURES.len() / 2;
let idx = 0;
for f in FEATURES {
    let p = "";
    p += "You are refactoring EXACTLY ONE feature directory of a Next.js client SPA at /Users/huangjiajiang/Development/erp.\n";
    p += "Your feature: erp-client/features/" + f + "\n";
    p += "This is a file-organization refactor. Hard requirement: the functional shape must not change — request paths, payloads, query keys, invalidation, form defaults/validators/submit, visible copy, CSS/layout, permission checks, and routing stay EXACTLY as they are today. Never rename exported symbols; never change props or hook signatures.\n";
    p += "You may create, move, delete, and edit files ONLY inside erp-client/features/" + f + "/. Never touch any file outside it — not even files that import your feature.\n";
    p += "PHASE 1 — Inventory (read-only):\n";
    p += "Read erp-client/AGENTS.md first and obey its conventions (client-only SPA: all requests via TanStack Query, all forms via TanStack Form, no SSR fetching).\n";
    p += "Run from /Users/huangjiajiang/Development/erp/erp-client to list every importer OUTSIDE your feature (BSD grep; rg is NOT installed):\n";
    p += "  grep -rln 'features/" + f + "/' --include='*.ts' --include='*.tsx' app components features lib hooks 2>/dev/null | grep -v 'features/" + f + "/'\n";
    p += "  grep -rln '../" + f + "/' --include='*.ts' --include='*.tsx' app components features lib hooks 2>/dev/null | grep -v 'features/" + f + "/'\n";
    p += "Every matched module path is a PUBLIC entry point: its exact path and named/default exports must keep working after your refactor, because you may not edit the importing files.\n";
    p += "List every file in your feature with line counts and classify each as api / hook / component / lib / types / mixed.\n";
    p += "PHASE 2 — Plan the self-describing layout (create only the directories you actually need):\n";
    p += "  api/        pure request functions and DTO mappers, no React\n";
    p += "  hooks/      React hooks only: TanStack Query hooks (from queries.ts), URL/search-param state, column-definition hooks\n";
    p += "  components/ presentational/interaction components that are not pages: dialogs, panels, columns, toolbars, previews, cards, forms; nest as pages/, list/, <domain>/ when such groupings exist\n";
    p += "  pages/      page-level components imported by app/ routes (*-page.tsx)\n";
    p += "  lib/        pure TS helpers: filters, url-state, validation, csv export, models, labels, formatting\n";
    p += "  types.ts    stays at the feature root (public module)\n";
    p += "Even for small features, api.ts code goes under api/ and queries.ts under hooks/ (with root re-export shims where imported externally). If the feature already has this structure (e.g. master-data, sales-orders), keep it and only make single-responsibility improvements that are clearly beneficial; do not churn working code.\n";
    p += "Split only files that mix responsibilities (UI mixed with query/form/url logic). Do not invent new behavior, new UI, new query keys, or new URL params. Files imported only inside the feature may move freely; externally imported files need a compatibility shim (PHASE 3).\n";
    p += "PHASE 3 — Execute:\n";
    p += "Use git mv when possible. After each move, update the feature-internal imports with grep (inside the feature only).\n";
    p += "NEVER edit files outside erp-client/features/" + f + "/ — preserve the public surface with re-export shims at the original paths instead. Example: moving customer-center-page.tsx into pages/ leaves customer-center-page.ts containing: export { CustomerCenterPage } from './pages/customer-center-page'. Use export type for type-only names and export { default } for default exports. Never leave an empty shim — re-export exactly the names importers use.\n";
    p += "Internal imports inside the feature must point at the new real locations, never through shims. Delete leftover empty directories.\n";
    p += "Do not rewrite business logic, JSX structure, styles, copy, query keys, or API payloads. Do not add dependencies; do not commit or push.\n";
    p += "PHASE 4 — Self-verify BEFORE reporting (mandatory):\n";
    p += "  cd /Users/huangjiajiang/Development/erp/erp-client\n";
    p += "  npx tsc --noEmit --incremental false — whole-repo check, may take minutes; run it once after your edits. Errors under features/" + f + "/ are yours to fix. Errors elsewhere count only when the erroring file imports your feature; otherwise ignore them (other agents edit in parallel).\n";
    p += "  npx oxlint features/" + f + " — must pass; fix real problems you introduced, never add disable comments.\n";
    p += "  npx oxfmt on each file you created or changed.\n";
    p += "Re-run the PHASE 1 grep commands and confirm each public path resolves to an existing file or shim exporting the right names.\n";
    p += "If an error in your feature survives two fix attempts, record it in errors and set ok=false; still fill public_entries, moves, and notes.\n";
    p += "Report schema: feature (string, exactly '" + f + "'), ok (boolean), public_entries (array of preserved external module paths, e.g. features/" + f + "/customer-center-page), moves (array of 'old -> new' strings), notes (string, short: what you moved/split/extracted), errors (array of strings, empty when clean).";
    let job = #{ prompt: p, label: "refactor:" + f, agent_type: "general-purpose", capability_mode: "all", output_schema: refactor_schema };
    if idx < half {
        jobs1.push(job);
    } else {
        jobs2.push(job);
    }
    idx += 1;
}
let r1 = [];
let r2 = [];
if jobs1.len() > 0 {
    r1 = parallel(jobs1);
}
log("refactor panel 1/2 done");
if jobs2.len() > 0 {
    r2 = parallel(jobs2);
}
log("refactor panel 2/2 done");

let all_results = [];
for x in r1 { all_results.push(x); }
for x in r2 { all_results.push(x); }
let okn = 0;
let failn = 0;
let rlines = [];
for r in all_results {
    if r != () && r.success && r.output != () && type_of(r.output) == "map" {
        let feat = r.output.feature;
        let okv = r.output.ok;
        let oks = if okv == true { "true" } else { "false" };
        if okv == true { okn += 1; } else { failn += 1; }
        let feat_s = if feat != () { feat } else { "(unnamed)" };
        rlines.push("  " + feat_s + ": ok=" + oks);
    } else {
        failn += 1;
        rlines.push("  (failed or empty slot)");
    }
}
log("refactor done: " + okn.to_string() + " ok / " + failn.to_string() + " not ok");

let VERIFY_PROMPT = "";
VERIFY_PROMPT += "You are the verification gate after all feature directories under erp-client/features/ were reorganized in parallel by other agents. Prove whole-repo integrity. Do not edit any file.\n";
VERIFY_PROMPT += "Run these commands and capture their full output:\n";
VERIFY_PROMPT += "  cd /Users/huangjiajiang/Development/erp/erp-client\n";
VERIFY_PROMPT += "  npx tsc --noEmit --incremental false\n";
VERIFY_PROMPT += "  npm run lint\n";
VERIFY_PROMPT += "  cd /Users/huangjiajiang/Development/erp && git status --porcelain\n";
VERIFY_PROMPT += "Classification:\n";
VERIFY_PROMPT += "- Typecheck or lint error in a file under erp-client/features/<name>/ => one problem with feature=<name>.\n";
VERIFY_PROMPT += "- Error in a file OUTSIDE features/: find which feature it imports (e.g. grep -n '@/features/' on that file) and attribute the problem to that feature.\n";
VERIFY_PROMPT += "- Any changed file not starting with erp-client/features/ (except erp-client/tsconfig.tsbuildinfo) => one problem with feature equal to the empty string, detail naming the paths.\n";
VERIFY_PROMPT += "- Pre-existing errors in files that were NOT changed by the refactor (check git status): ignore; count them in notes.\n";
VERIFY_PROMPT += "Merge all errors of one feature into ONE problem entry; its detail holds at most 4 representative one-line messages plus file paths. A valid empty problems array requires that you really ran all three checks.\n";
VERIFY_PROMPT += "Return: passed (boolean, true only if problems is empty), problems (array of {feature: string, detail: string}), notes (string with counts).";

phase("Verify");
let vr = agent(VERIFY_PROMPT, #{ label: "verify:typecheck", agent_type: "general-purpose", capability_mode: "execute", output_schema: verify_schema });
let problems = [];
if vr != () && vr.success && vr.output != () && type_of(vr.output) == "map" {
    let ps = vr.output.problems;
    if ps != () && type_of(ps) == "array" {
        for p in ps {
            if p != () && type_of(p) == "map" { problems.push(p); }
        }
    }
} else {
    problems.push(#{ feature: "", detail: "verify agent failed or produced no output" });
}
log("verify: " + problems.len().to_string() + " problem groups");

let seen = [];
let targeted = [];
for p in problems {
    let f = p.feature;
    if f != () && f != "" {
        let known = false;
        for x in FEATURES { if x == f { known = true; } }
        if known {
            let dup = false;
            for s in seen { if s == f { dup = true; } }
            if !dup {
                seen.push(f);
                targeted.push(p);
            }
        }
    } else {
        targeted.push(p);
    }
}
log("targeted fixes: " + targeted.len().to_string());

phase("Fix");
let fjobs = [];
for p in targeted {
    if p != () && p.feature != () && p.feature != "" {
        let fp = "";
        fp += "You are fixing regressions in ONE feature after a file-organization refactor: erp-client/features/" + p.feature + ".\n";
        fp += "Reported problems from whole-repo typecheck/lint:\n";
        fp += json_encode(p.detail) + "\n\n";
        fp += "Rules: edit files ONLY inside erp-client/features/" + p.feature + "/. Preserve the public surface — every module path imported from outside the feature must still resolve with the same named/default exports (re-export shims at original paths are fine). Never rename exports; never change props, query keys, schemas, or strings.\n";
        fp += "Procedure: cd /Users/huangjiajiang/Development/erp/erp-client; run npx tsc --noEmit --incremental false; fix errors related to your feature (a file outside your feature counts only if it imports your feature); re-run until your feature is clean; then npx oxlint features/" + p.feature + " and npx oxfmt on the files you changed. Do not commit or push.\n";
        fp += "Return: feature (string), fixed (boolean, true only after your feature is clean in tsc and oxlint), notes (string).";
        fjobs.push(#{ prompt: fp, label: "fix:" + p.feature, agent_type: "general-purpose", capability_mode: "all", output_schema: fixer_schema });
    }
}
let fixlines = [];
if fjobs.len() > 0 {
    let frs = parallel(fjobs);
    for r in frs {
        if r != () && r.success && r.output != () && type_of(r.output) == "map" {
            let feat = r.output.feature;
            let fx = if r.output.fixed == true { "fixed" } else { "still broken" };
            let feat_s = if feat != () { feat } else { "(unnamed)" };
            fixlines.push("  " + feat_s + ": " + fx);
        } else {
            fixlines.push("  (failed fixer slot)");
        }
    }
}
log("fix panel done: " + fjobs.len().to_string() + " fixers");

let FINAL_PROMPT = "";
FINAL_PROMPT += "Final verification gate. Do not edit any file; only run checks and report.\n";
FINAL_PROMPT += "  cd /Users/huangjiajiang/Development/erp/erp-client\n";
FINAL_PROMPT += "  1. npx tsc --noEmit --incremental false\n";
FINAL_PROMPT += "  2. npm run lint\n";
FINAL_PROMPT += "  3. cd /Users/huangjiajiang/Development/erp && git status --porcelain — only paths starting with erp-client/features/ may be changed (erp-client/tsconfig.tsbuildinfo is acceptable).\n";
FINAL_PROMPT += "Return: passed (boolean, true only when all three are clean), summary (string, one line with counts of type errors, lint errors, and scope violations), remaining (array of strings, one per remaining error as file:line message; empty when clean).";

phase("Final verify");
let fv = agent(FINAL_PROMPT, #{ label: "verify:final", agent_type: "general-purpose", capability_mode: "execute", output_schema: final_schema });
let final_passed = false;
let final_summary = "final verify produced no usable output";
let remaining = [];
if fv != () && fv.success && fv.output != () && type_of(fv.output) == "map" {
    if fv.output.passed == true { final_passed = true; }
    if fv.output.summary != () { final_summary = fv.output.summary; }
    let rem = fv.output.remaining;
    if rem != () && type_of(rem) == "array" { remaining = rem; }
}

let report = "# erp-client feature refactor — report\n\n";
report += "Features processed: " + FEATURES.len().to_string() + "\n\n";
report += "## Refactor results\n";
for l in rlines { report += l + "\n"; }
report += "\n## Whole-repo verification\n";
report += "problem groups found: " + problems.len().to_string() + "; targeted fixes: " + targeted.len().to_string() + "\n";
for p in problems {
    let ff = if p.feature != () { p.feature } else { "(scope)" };
    let dd = if p.detail != () { p.detail } else { "(no detail)" };
    report += "- feature: " + ff + " — " + dd + "\n";
}
report += "\n## Fix panel\n";
for l in fixlines { report += l + "\n"; }
report += "\n## Final verification\n";
report += "passed: " + final_passed.to_string() + "\n";
report += final_summary + "\n";
if remaining.len() > 0 {
    report += "Remaining errors:\n";
    for e in remaining { report += "- " + e + "\n"; }
}
let report_path = write_scratch_file("refactor-all-features-report.md", report);
complete(#{ path: report_path, report: report });
