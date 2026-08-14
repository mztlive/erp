let meta = #{
    name: "refactor-erp-features",
    description: "Refactor every feature under erp-client/features: split large files, extract components and hooks, add vitest hook tests, then verify with lint/typecheck/tests/build and fix rounds.",
    when_to_use: "Run when erp-client/features needs a systematic per-feature refactor (file splitting, component/hook extraction, hook test coverage) with a project-wide verification gate.",
    phases: [
        #{ title: "Setup", detail: "install vitest + testing-library infra" },
        #{ title: "Refactor", detail: "one agent per feature shard, 55 jobs" },
        #{ title: "Verify", detail: "lint, tsc, vitest, next build" },
        #{ title: "Fix", detail: "up to 2 targeted fix rounds" },
    ],
};

let refactor_schema = #{
    "type": "object",
    "required": ["summary", "files_created", "files_modified", "tests_added", "tests_run", "tests_failed"],
    "properties": #{
        "summary": #{ "type": "string" },
        "files_created": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "files_modified": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "tests_added": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "tests_run": #{ "type": "integer" },
        "tests_failed": #{ "type": "integer" },
        "blockers": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
    },
};

let setup_schema = #{
    "type": "object",
    "required": ["summary", "deps_installed", "files_created", "sanity_result"],
    "properties": #{
        "summary": #{ "type": "string" },
        "deps_installed": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
        "files_created": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
        "sanity_result": #{ "type": "string" },
        "vitest_version": #{ "type": "string" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["gates", "failed_test_files", "summary"],
    "properties": #{
        "gates": #{
            "type": "array", "maxItems": 6,
            "items": #{
                "type": "object",
                "required": ["name", "pass", "detail"],
                "properties": #{
                    "name": #{ "type": "string" },
                    "pass": #{ "type": "boolean" },
                    "detail": #{ "type": "string" },
                },
            },
        },
        "failed_test_files": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let fix_schema = #{
    "type": "object",
    "required": ["fixed", "summary", "remaining_errors"],
    "properties": #{
        "fixed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "remaining_errors": #{ "type": "string" },
    },
};

const setup_prompt = `You are setting up test infrastructure for an ERP SPA. Repo root: /Users/huangjiajiang/Development/erp. App dir: erp-client (Next.js 16 App Router used as a pure client SPA, React 19, TypeScript strict, TanStack Query v5). A large refactoring workflow will soon add vitest hook tests under erp-client/features/<feature>/hooks/. Your job: install and configure vitest + React Testing Library, add a shared test helper, and prove it works with a sanity test.

STEPS:
1. Read erp-client/package.json and erp-client/tsconfig.json first.
2. From erp-client run: npm install -D vitest jsdom @testing-library/react @testing-library/dom
3. Create erp-client/vitest.config.mts:
   - resolve.alias: '@' pointing at the erp-client directory (use fileURLToPath(new URL('.', import.meta.url))).
   - test.environment: 'jsdom'; test.include: ['features/**/*.test.{ts,tsx}', 'tests/**/*.test.{ts,tsx}']; test.cache: false; test.globals: false; esbuild jsx: 'automatic'.
   - Do NOT include *.test.mts files (those run under node --test separately).
4. Create erp-client/features/test-utils.tsx (a shared client helper, not a test file):
   - export function createFreshQueryClient(): new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } }).
   - export function renderHookWithProviders(callback, options?): wraps callback in QueryClientProvider (fresh client unless options.queryClient is given) and calls renderHook from @testing-library/react, returning its result. Keep it tiny; no UI code, no feature imports.
5. Add npm scripts to erp-client/package.json:
   - "test": "vitest run"
   - "test:watch": "vitest"
   - "test:node": "node --test --experimental-strip-types \"features/**/*.test.mts\" \"scripts/*.test.mts\"" — verify this exact command works on the installed Node; if the glob form fails, make the script correct (e.g. find-based). Keep the existing "test:api-errors" script unchanged.
6. Create erp-client/tests/infra-sanity.test.tsx: define a tiny useCounter hook and a tiny useQuery-based hook inline in the test file, test both with renderHook and renderHookWithProviders, then run: npx vitest run tests/infra-sanity.test.tsx — it must pass.
7. Verify: npx tsc --noEmit --incremental false and npx oxlint tests vitest.config.mts features/test-utils.tsx — fix any errors you introduced.
8. Do NOT modify anything else under erp-client/features (other than creating features/test-utils.tsx), do NOT run next build, do NOT git commit.

Report deps_installed, files_created (repo-relative), sanity_result (command + pass/fail), vitest_version, and a summary.`;

const context_block = `You are one agent in a large parallel refactoring workflow for an ERP SPA. Repo root: /Users/huangjiajiang/Development/erp. The app is erp-client: Next.js 16 App Router used as a pure client-side SPA ("use client" business pages), React 19, TypeScript strict, TanStack Query v5 for ALL server data, TanStack Form for all forms, shadcn/Base UI components in erp-client/components/ui.

MANDATORY before editing: read erp-client/AGENTS.md fully (SPA-only rules, TanStack Query rules, form rules, terminology rules). If you touch any user-visible string, first read /Users/huangjiajiang/Development/erp/docs/ui-glossary.md. Next.js 16 has breaking changes vs your training data; when unsure about a Next API, read the relevant guide under erp-client/node_modules/next/dist/docs/.

This is a BEHAVIOR-PRESERVING refactor. You must NOT change: user-visible strings, API request shapes, URL query params and their semantics, TanStack Query keys, form validation rules, or any business behavior. Split files, extract components and hooks, add tests — nothing else.

Test infrastructure is already installed (vitest, jsdom, @testing-library/react) with helpers in erp-client/features/test-utils.tsx. Do NOT run npm install.`;

const tasks_block = `TASKS (read all your in-scope files first, then do what applies):

1. SPLIT LARGE FILES. Every file in your scope over ~400 lines must be split at cohesive boundaries. Extract UI blocks into components/ as named components with explicit typed props; extract stateful logic into hooks/ (names start with "use"); extract pure helpers into lib/. After splitting, no production file you touch should stay over ~400 lines, except declarative data tables (huge const arrays / config maps), which may remain with a short comment saying why. For very large api/ modules: split into per-resource files and re-export everything from the original module so all existing imports keep working. Every public export of a file you split must keep the same name, signature, and behavior.

2. EXTRACT COMPONENTS. Repeated JSX and long inline sections (tables, dialogs, panels, filter bars, form sections) become components in components/ with typed props. Rendering must stay identical.

3. EXTRACT HOOKS. useState/useEffect/useMemo/useCallback clusters, URL-state management, list/table state, form default derivation -> hooks in hooks/. Data fetching must remain TanStack Query hooks (queries.ts); never convert it to useEffect + fetch. Keep queryKey structures unchanged.

4. HOOK TESTS (required for every hook in scope): for every existing hook and every new hook you extract, write a vitest test file colocated with it named <file>.test.ts(x) (e.g. hooks/queries.ts -> hooks/queries.test.ts). Every testable hook in scope must be exercised; a test file may cover several hooks.`;

const tests_block = `Test guidelines:
- Imports: import { describe, it, expect, vi, beforeEach } from 'vitest'; import { renderHook, act, waitFor } from '@testing-library/react'.
- Use the shared helpers from erp-client/features/test-utils.tsx (renderHookWithProviders, createFreshQueryClient) for TanStack Query hooks: a fresh QueryClient per test. Do NOT edit that shared file; if something is missing, create a local helper inside your feature.
- If a hook imports from 'next/navigation' (useRouter/useSearchParams/usePathname/useParams), add at the top of the test file: vi.mock('next/navigation', () => ({ useRouter: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }), useSearchParams: () => new URLSearchParams(), usePathname: () => '/test', useParams: () => ({}) })); and use vi.mocked(...) to drive per-test values.
- Query hooks: vi.mock the api module to return canned data; assert the right queryFn args, pending/error/data states, and key stability. Mutation hooks: assert mutationFn wiring and that success flows call the api.
- URL-state hooks: test defaults, param parsing, and setters producing the expected URL updates (mock window.history.pushState / replaceState).
- Test edge cases: empty data, error responses, boundary values. Assert observable behavior, not implementation details. No snapshot tests. Meaningful assertions over trivial ones.
- Do not test Next.js page components (App Router page files). If a page still embeds logic you could not safely extract, note it in blockers.`;

const verify_block = `VERIFY before finishing (run from /Users/huangjiajiang/Development/erp/erp-client):
- npx vitest run <your test files> — all must pass; rerun and fix until green.
- npx oxlint <each file you touched> — fix issues you introduced (you may run npx oxlint --fix on your files only).
- Optional: npx tsc --noEmit --incremental false (slow; a central gate runs it later anyway).
- NEVER: npm install, npm run build, next build, npx vitest run with no file args (the full suite belongs to the orchestrator), any git write command, or edits outside your scope. If you must change a file outside your scope, do not; record it in blockers instead.`;

const output_block = `OUTPUT: report exactly the required JSON fields. Paths are repo-relative (erp-client/...). files_created / files_modified: production files you created/changed; tests_added: the test files you wrote; tests_run: how many tests you ran; tests_failed: must be 0 when you finish; summary: 2-4 sentences (what you split, extracted, tested); blockers: what you could not do and why.
If after reading the code there is genuinely nothing to split (no file over 400 lines), nothing worth extracting, and no testable hooks, make NO production changes and say so in summary with evidence (largest file size, list of hooks found). Do not invent refactors to look busy.`;

const verify_prompt = `You are the verification gate for a large parallel refactor of erp-client (Next.js 16 App Router SPA) at /Users/huangjiajiang/Development/erp/erp-client. Many agents just refactored feature directories under erp-client/features and added vitest tests. Your job: run the project-wide gates and report exact failures.

From /Users/huangjiajiang/Development/erp/erp-client run these gates in order:
1. lint: npm run lint
2. typecheck: npx tsc --noEmit --incremental false
3. tests: npx vitest run
4. build (ONLY if gates 1-3 all passed): npm run build
For each gate that fails, capture the first ~40 lines of its output into that gate's detail field. For failing tests, collect every failing test file path as repo-relative (erp-client/features/... form) into failed_test_files (max 60; if there are more, take the first 60 and say so in summary). Be precise and do NOT fix anything yourself.

Output gates: one entry per gate with name/pass/detail for lint, typecheck, tests, build. If the build was skipped because an earlier gate failed, report build as pass=false with detail "skipped because an earlier gate failed". If gates 1-3 all passed, the build must actually run and be reported with its real result. failed_test_files: empty array when all tests pass. summary: one sentence of overall status.`;

const verify_rerun_prompt = `You previously ran the project-wide verification gates for the ERP refactor (lint, typecheck, tests, build) at /Users/huangjiajiang/Development/erp/erp-client. A fix round has just completed. Re-run the same gates from /Users/huangjiajiang/Development/erp/erp-client exactly as before (lint, then typecheck, then npx vitest run, then npm run build only if the first three passed) and produce the same structured output: gates (name/pass/detail for lint, typecheck, tests, build), failed_test_files (repo-relative erp-client/... paths of failing test files), and a one-sentence summary. Do not fix anything yourself.`;

const generic_fix_prompt = `You are fixing project-wide errors introduced by a parallel feature refactor in the ERP SPA. Repo root: /Users/huangjiajiang/Development/erp. App: erp-client (Next.js 16 SPA, React 19, TanStack Query v5). Read erp-client/AGENTS.md first. These gates failed — error excerpts:

<ERR_EXCERPT>

Rules: fix ONLY files under erp-client/features/ — and only errors the refactor plausibly introduced (in refactored feature files). Leave clearly pre-existing or infrastructure errors untouched and note them in remaining_errors. Verify with npx tsc --noEmit --incremental false and/or npx oxlint on the changed files, run from erp-client. Never touch package.json, configs, other directories, or git.

Output: fixed (true only if every error excerpt you were given is resolved or determined pre-existing), summary, remaining_errors (empty string if none remain).`;

fn feature_of(p) {
    let idx = p.index_of("features/");
    if idx < 0 { return "shared"; }
    let rest = p.sub_string(idx + 9, p.len() - idx - 9);
    let parts = rest.split("/");
    if parts.len() == 0 { return "shared"; }
    parts[0]
}

fn fix_prompt(feat, files, errs) {
    let p = "You are fixing regressions from a feature refactor in the ERP SPA. Repo root: /Users/huangjiajiang/Development/erp. App: erp-client (Next.js 16 SPA, React 19, TanStack Query v5). Read erp-client/AGENTS.md first.\n";
    p += "Feature area: " + feat + "\n";
    p += "Failing test files (repo-relative): " + files + "\n";
    p += "Additional error excerpt (may be empty): " + errs + "\n";
    p += "Rules: edit only files under erp-client/features/" + feat + "/ (tests and production). Fix the production code; only change a test if the test itself is wrong. Rerun npx vitest run <each failing file> from erp-client until all pass, and npx oxlint on the changed files. If an error is clearly pre-existing infra noise unrelated to the refactor, do not chase it — report it in remaining_errors. Never touch package.json, configs, other features, or git.\n";
    p += "Output: fixed (true only if every listed test now passes), summary, remaining_errors (empty string if none).";
    p
}

phase("Setup");
let su = agent(setup_prompt, #{ label: "setup:test-infra", capability_mode: "all", output_schema: setup_schema });
if su == () || su.success == false {
    complete(#{ status: "failed", stage: "setup", detail: "Test infrastructure setup agent failed; no refactor jobs were launched." });
}

phase("Refactor");
let jobs = [];

const simple_features = "actual-profit-loss,admin,auth,card-business-analytics,card-funds-review,contracts,customer-quality,entity-selectors,execution-projections,file-assets,history-backfill,import-opening,supplier-api-connections,supplier-offerings,unified-task-queue,work-items,workspace";
for f in simple_features.split(",") {
    jobs.push(#{ label: f, feature: "erp-client/features/" + f, files: "ALL files under the feature directory (every file, no exceptions)", prefixes: "any path under the feature directory" });
}

jobs.push(#{ label: "access-audit:page", feature: "erp-client/features/access-audit", files: "pages/access-audit-page.tsx, access-audit-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "access-audit:rest", feature: "erp-client/features/access-audit", files: "api/index.ts, components/, hooks/, lib/, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "customer-receivables:page", feature: "erp-client/features/customer-receivables", files: "pages/customer-receivables-page.tsx, customer-receivables-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "customer-receivables:rest", feature: "erp-client/features/customer-receivables", files: "api/index.ts, components/, hooks/, lib/, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "customers:detail", feature: "erp-client/features/customers", files: "pages/customer-detail-page.tsx, customer-detail-page.ts, components/customer-form.tsx", prefixes: "pages/customer-detail-, pages/customer-form-, components/customer-form-, components/customer-detail-, hooks/use-customer-detail-, hooks/use-customer-form-" });
jobs.push(#{ label: "customers:rest", feature: "erp-client/features/customers", files: "api.ts, api/, components/customer-assignment-dialog.tsx, components/customer-create-dialog.tsx, hooks/, lib/, pages/customer-center-page.tsx, customer-center-page.ts, queries.ts, types.ts", prefixes: "api/, components/customer-assignment-, components/customer-create-, hooks/use-directory-, hooks/use-customer-center-, lib/, pages/customer-center-" });

jobs.push(#{ label: "fulfillment-operations:page", feature: "erp-client/features/fulfillment-operations", files: "pages/fulfillment-operations-page.tsx, fulfillment-operations-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "fulfillment-operations:rest", feature: "erp-client/features/fulfillment-operations", files: "api/index.ts, components/, hooks/, lib/, queries.ts, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "integration-errors:page", feature: "erp-client/features/integration-errors", files: "pages/integration-errors-page.tsx, integration-errors-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "integration-errors:rest", feature: "erp-client/features/integration-errors", files: "api/, components/, hooks/, lib/, pages/integration-error-detail-page.tsx, integration-error-detail-page.ts, types.ts", prefixes: "api/, components/, hooks/, lib/, pages/integration-error-detail-" });

jobs.push(#{ label: "inventory:page", feature: "erp-client/features/inventory", files: "pages/inventory-ledger-page.tsx, inventory-ledger-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "inventory:rest", feature: "erp-client/features/inventory", files: "api.ts, api/, components/, hooks/, lib/, presentation.tsx, queries.ts, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "mall-consumption-orders:center", feature: "erp-client/features/mall-consumption-orders", files: "pages/consumption-order-center-page.tsx, consumption-order-center-page.ts", prefixes: "pages/consumption-order-center-" });
jobs.push(#{ label: "mall-consumption-orders:rest", feature: "erp-client/features/mall-consumption-orders", files: "api/, components/, hooks/, lib/, pages/consumption-orders-list-page.tsx, consumption-orders-list-page.ts, queries.ts, types.ts", prefixes: "api/, components/, hooks/, lib/, pages/consumption-orders-list-" });

jobs.push(#{ label: "mall-sync:page", feature: "erp-client/features/mall-sync", files: "pages/mall-sync-page.tsx, mall-sync-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "mall-sync:rest", feature: "erp-client/features/mall-sync", files: "api.ts, api/index.ts, components/, hooks/, lib/, queries.ts, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "master-data:supplier", feature: "erp-client/features/master-data", files: "components/supplier/, components/brand/, components/pages/suppliers-list-page.tsx, components/pages/brands-list-page.tsx, components/list/supplier-list-toolbar.tsx, hooks/use-supplier-editor.ts, hooks/use-supplier-list-columns.tsx, hooks/use-supplier-list-filters.ts, hooks/use-supplier-list-state.ts, lib/supplier-editor-model.ts", prefixes: "components/supplier/, components/brand/, components/pages/suppliers-list-, components/pages/brands-list-, components/list/supplier-, hooks/use-supplier-, lib/supplier-" });
jobs.push(#{ label: "master-data:product", feature: "erp-client/features/master-data", files: "components/product/, components/unit-of-measure/, components/pages/products-list-page.tsx, components/pages/sellable-items-list-page.tsx, components/list/sellable-list-toolbar.tsx, components/list/sellable-preview-sheet.tsx, components/list/product-list-toolbar.tsx, hooks/use-product-editor.ts, hooks/use-product-list-columns.tsx, hooks/use-product-list-filters.ts, hooks/use-product-list-state.ts, hooks/use-sellable-list-columns.tsx, hooks/use-sellable-list-filters.ts, hooks/use-sellable-list-state.ts, lib/product-editor-model.ts, lib/product-model.ts, lib/resource-fields.ts", prefixes: "components/product/, components/unit-of-measure/, components/pages/products-list-, components/pages/sellable-items-list-, components/list/sellable-, components/list/product-, hooks/use-product-, hooks/use-sellable-, lib/product-, lib/resource-fields" });
jobs.push(#{ label: "master-data:list", feature: "erp-client/features/master-data", files: "components/category/, components/shared/, components/warehouse/, components/list/dictionary-list-toolbar.tsx, components/list/lifecycle-metric-strip.tsx, components/list/list-chrome.tsx, components/list/list-column-primitives.tsx, components/list/list-page-frame.tsx, components/list/list-search-field.tsx, components/list/list-toolbar-actions.tsx, components/list/master-data-preview.tsx, components/list/voucher-category-form-dialog.tsx, components/list/warehouse-preview-sheet.tsx, components/pages/list-route-redirect.tsx, components/pages/unit-of-measures-list-page.tsx, components/pages/unknown-master-data-page.tsx, components/pages/voucher-categories-list-page.tsx, components/pages/warehouses-list-page.tsx, hooks/use-client-paged-rows.ts, hooks/use-create-permission.ts, hooks/use-dictionary-list-columns.tsx, hooks/use-dictionary-list-state.ts, hooks/use-lifecycle-list-filters.ts, hooks/use-list-page-chrome.ts, hooks/use-list-url.ts, hooks/use-master-data-list-export.ts, hooks/use-slash-search-hotkey.ts, lib/category-tree-model.ts, lib/copy.ts, lib/data.ts, lib/export-csv.ts, lib/filter.ts, lib/list-filters.ts, lib/master-data-list-summaries.ts, lib/move-list-item.ts, lib/revision-target.ts", prefixes: "components/category/, components/shared/, components/warehouse/, components/list/dictionary-, components/list/lifecycle-, components/list/list-, components/list/master-data-, components/list/voucher-, components/list/warehouse-, components/pages/list-route-, components/pages/unit-of-measures-, components/pages/unknown-, components/pages/voucher-categories-, components/pages/warehouses-, hooks/use-client-paged-, hooks/use-create-permission-, hooks/use-dictionary-, hooks/use-lifecycle-, hooks/use-list-, hooks/use-master-data-, hooks/use-slash-, lib/category-tree-, lib/copy-, lib/data-, lib/export-csv-, lib/filter-, lib/list-filters-, lib/master-data-list-, lib/move-list-, lib/revision-" });
jobs.push(#{ label: "master-data:api", feature: "erp-client/features/master-data", files: "api.ts, api/, types.ts, hooks/queries.ts", prefixes: "api/, hooks/queries-, hooks/use-api-" });

jobs.push(#{ label: "procurement-confirmation:page", feature: "erp-client/features/procurement-confirmation", files: "pages/procurement-confirmation-page.tsx, procurement-confirmation-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "procurement-confirmation:rest", feature: "erp-client/features/procurement-confirmation", files: "api/index.ts, components/, hooks/, lib/, types.ts", prefixes: "api/, components/, hooks/, lib/" });

jobs.push(#{ label: "product-publications:center", feature: "erp-client/features/product-publications", files: "pages/publication-center-page.tsx, publication-center-page.ts", prefixes: "pages/publication-center-" });
jobs.push(#{ label: "product-publications:rest", feature: "erp-client/features/product-publications", files: "api.ts, api/, components/, hooks/, lib/, pages/product-publications-list-page.tsx, product-publications-list-page.ts, queries.ts, types.ts", prefixes: "api/, components/, hooks/, lib/, pages/product-publications-list-" });

jobs.push(#{ label: "purchase-orders:detail", feature: "erp-client/features/purchase-orders", files: "pages/purchase-order-detail-page.tsx, purchase-order-detail-page.ts, lib/purchase-order-validation.ts", prefixes: "pages/purchase-order-detail-, components/purchase-order-detail-, hooks/use-purchase-order-detail-, lib/purchase-order-validation-" });
jobs.push(#{ label: "purchase-orders:rest", feature: "erp-client/features/purchase-orders", files: "pages/purchase-orders-list-page.tsx, purchase-orders-list-page.ts, components/creation-basis-search-combobox.tsx, components/purchase-order-preview-panel.tsx, components/purchase-order-surfaces.tsx, hooks/queries.ts, lib/url-state.ts, api/purchase-orders.ts, queries.ts, types.ts", prefixes: "pages/purchase-orders-list-, components/creation-basis-, components/purchase-order-preview-, components/purchase-order-surfaces-, hooks/use-purchase-orders-, lib/purchase-orders-, api/" });

jobs.push(#{ label: "sales-orders:create", feature: "erp-client/features/sales-orders", files: "components/sales-order-create-form.tsx, pages/sales-order-create-page.tsx, lib/sales-order-create-model.ts, api/card-approval.ts, api/contracts.ts", prefixes: "components/sales-order-create-, pages/sales-order-create-, hooks/use-sales-order-create-, lib/sales-order-create-" });
jobs.push(#{ label: "sales-orders:acceptance", feature: "erp-client/features/sales-orders", files: "components/acceptance-workspace.tsx, hooks/acceptance-url-state.ts, lib/acceptance-types.ts, api/acceptance.ts", prefixes: "components/acceptance-, hooks/use-acceptance-, lib/acceptance-" });
jobs.push(#{ label: "sales-orders:detail", feature: "erp-client/features/sales-orders", files: "pages/sales-order-detail-page.tsx, components/sales-order-detail-panels.tsx, lib/sales-order-detail-model.ts, api/mappers.ts", prefixes: "pages/sales-order-detail-, components/sales-order-detail-, hooks/use-sales-order-detail-, lib/sales-order-detail-" });
jobs.push(#{ label: "sales-orders:list", feature: "erp-client/features/sales-orders", files: "pages/sales-orders-list-page.tsx, lib/filter-orders.ts, lib/url-state.ts, lib/labels.ts, api/export.ts", prefixes: "pages/sales-orders-list-, components/sales-orders-list-, hooks/use-sales-orders-list-, lib/filter-, lib/labels-, lib/sales-orders-list-" });
jobs.push(#{ label: "sales-orders:rest", feature: "erp-client/features/sales-orders", files: "components/card-sales-approval-panel.tsx, components/close-conditions-card.tsx, components/low-margin-manager-panel.tsx, components/procurement-rejection-card.tsx, components/revision-history-card.tsx, components/sales-change-review-panel.tsx, components/sales-order-paper-dialog.tsx, components/void-sales-order-dialog.tsx, api/sales-orders.ts, api.ts, hooks/queries.ts, types.ts", prefixes: "components/card-sales-approval-, components/close-conditions-, components/low-margin-, components/procurement-rejection-, components/revision-history-, components/sales-change-, components/sales-order-paper-, components/void-sales-order-, hooks/use-card-approval-, hooks/use-low-margin-, hooks/use-change-review-, hooks/use-revision-, api/sales-orders-, api/queries-" });

jobs.push(#{ label: "supplier-orders:center", feature: "erp-client/features/supplier-orders", files: "pages/supplier-order-center-page.tsx, supplier-order-center-page.ts, components/supplier-order-preview-panel.tsx", prefixes: "pages/supplier-order-center-, components/supplier-order-preview-, hooks/use-supplier-order-center-" });
jobs.push(#{ label: "supplier-orders:rest", feature: "erp-client/features/supplier-orders", files: "pages/supplier-orders-list-page.tsx, supplier-orders-list-page.ts, hooks/queries.ts, hooks/use-supplier-orders-list-columns.tsx, lib/url-state.ts, api/index.ts, api.ts, queries.ts, types.ts", prefixes: "pages/supplier-orders-list-, components/supplier-orders-list-, hooks/use-supplier-orders-, api/, lib/supplier-orders-" });

jobs.push(#{ label: "supplier-payables:page", feature: "erp-client/features/supplier-payables", files: "pages/supplier-accounts-page.tsx, supplier-accounts-page.ts", prefixes: "pages/" });
jobs.push(#{ label: "supplier-payables:alloc", feature: "erp-client/features/supplier-payables", files: "components/allocation-session.tsx, lib/allocation-model.ts", prefixes: "components/allocation-, hooks/use-allocation-, lib/allocation-" });
jobs.push(#{ label: "supplier-payables:rest", feature: "erp-client/features/supplier-payables", files: "api.ts, api/, hooks/queries.ts, hooks/use-supplier-accounts-columns.tsx, lib/url-state.ts, queries.ts, types.ts", prefixes: "api/, hooks/use-supplier-accounts-, hooks/use-supplier-payables-, lib/supplier-accounts-" });

jobs.push(#{ label: "supplier-settlements:center", feature: "erp-client/features/supplier-settlements", files: "components/settlement-center.tsx", prefixes: "components/settlement-center-, components/settlement-, hooks/use-settlement-, lib/settlement-" });
jobs.push(#{ label: "supplier-settlements:rest", feature: "erp-client/features/supplier-settlements", files: "api.ts, api/settlements.ts, components/create-draft-dialog.tsx, components/cross-entry-banner.tsx, components/differences-workspace.tsx, components/settlement-list.tsx, hooks/queries.ts, hooks/use-settlement-list-columns.tsx, hooks/use-settlement-list-search-hotkey.ts, lib/operations.ts, lib/url-state.ts, operations.ts, pages/supplier-settlements-page.tsx, queries.ts, supplier-settlements-page.ts, types.ts, url-state.ts", prefixes: "api/, components/create-draft-, components/cross-entry-, components/differences-, components/settlement-list-, hooks/use-settlement-list-, lib/operations-, lib/supplier-settlements-, pages/" });

log("Launching " + jobs.len().to_string() + " refactor jobs in parallel");

let job_specs = [];
for j in jobs {
    let p = context_block;
    p += "\n\n";
    p += "YOUR SCOPE (feature area label: " + j.label + ")\n";
    p += "Feature directory: " + j.feature + "\n";
    p += "Files you may edit, relative to the feature directory (ONLY these):\n" + j.files + "\n";
    p += "New files you create must be under one of these paths (relative to the feature directory):\n" + j.prefixes + "\n";
    p += "Test files are colocated with the hook file they test and are always allowed. Never edit anything outside this scope.\n";
    p += "\n\n" + tasks_block;
    p += "\n\n" + tests_block;
    p += "\n\n" + verify_block;
    p += "\n\n" + output_block;
    job_specs.push(#{ prompt: p, label: "refactor:" + j.label, capability_mode: "all", output_schema: refactor_schema });
}
let results = parallel(job_specs);

let retry_specs = [];
let retry_idxs = [];
let i = 0;
for r in results {
    if r == () || r.success == false {
        let j = jobs[i];
        let p = context_block;
        p += "\n\n";
        p += "YOUR SCOPE (feature area label: " + j.label + ")\n";
        p += "Feature directory: " + j.feature + "\n";
        p += "Files you may edit, relative to the feature directory (ONLY these):\n" + j.files + "\n";
        p += "New files you create must be under one of these paths (relative to the feature directory):\n" + j.prefixes + "\n";
        p += "Test files are colocated with the hook file they test and are always allowed. Never edit anything outside this scope.\n";
        p += "\n\n" + tasks_block;
        p += "\n\n" + tests_block;
        p += "\n\n" + verify_block;
        p += "\n\n" + output_block;
        retry_specs.push(#{ prompt: p, label: "refactor-retry:" + j.label, capability_mode: "all", output_schema: refactor_schema });
        retry_idxs.push(i);
    }
    i += 1;
}
if retry_specs.len() > 0 {
    log("Retrying " + retry_specs.len().to_string() + " failed refactor jobs");
    let retry_results = parallel(retry_specs);
    let k = 0;
    for idx in retry_idxs {
        if retry_results[k] != () && retry_results[k].success == true {
            results[idx] = retry_results[k];
        }
        k += 1;
    }
}

phase("Verify");
let v1 = agent(verify_prompt, #{ label: "verify:round1", capability_mode: "execute", output_schema: verify_schema });

let rounds = 0;
while rounds < 2 {
    let all_pass = false;
    let failed = [];
    let terr = "";
    let lerr = "";
    if v1 != () && v1.success == true && v1.output.gates != () {
        all_pass = true;
        for g in v1.output.gates {
            if g.pass != true {
                all_pass = false;
                if g.name == "typecheck" && g.detail != () { terr = g.detail; }
                if g.name == "lint" && g.detail != () { lerr = g.detail; }
            }
        }
        if v1.output.failed_test_files != () {
            for f in v1.output.failed_test_files { failed.push(f); }
        }
    }
    if all_pass == true {
        rounds = 2;
    } else {
        phase("Fix");
        let feats = [];
        let flists = [];
        for f in failed {
            let feat = feature_of(f);
            let found = -1;
            let m = 0;
            for ff in feats {
                if ff == feat { found = m; }
                m += 1;
            }
            if found < 0 {
                feats.push(feat);
                flists.push(f);
            } else {
                flists[found] = flists[found] + "\n" + f;
            }
        }
        let fix_jobs = [];
        let cap = 0;
        let n = 0;
        for feat in feats {
            if cap < 8 {
                fix_jobs.push(#{ prompt: fix_prompt(feat, flists[n], ""), label: "fix:" + feat + ":" + rounds.to_string(), capability_mode: "all", output_schema: fix_schema });
                cap += 1;
            }
            n += 1;
        }
        if feats.len() > 8 {
            log("Dropped " + (feats.len() - 8).to_string() + " fix jobs (cap 8 per round)");
        }
        if terr != "" || lerr != "" {
            let excerpts = "";
            if terr != "" { excerpts += "typecheck errors:\n" + terr.sub_string(0, 1500) + "\n\n"; }
            if lerr != "" { excerpts += "lint errors:\n" + lerr.sub_string(0, 1500) + "\n"; }
            let gp = generic_fix_prompt;
            let at = gp.index_of("<ERR_EXCERPT>");
            if at >= 0 {
                gp = gp.sub_string(0, at) + excerpts + gp.sub_string(at + 13, gp.len() - at - 13);
            }
            fix_jobs.push(#{ prompt: gp, label: "fix:gates:" + rounds.to_string(), capability_mode: "all", output_schema: fix_schema });
        }
        if fix_jobs.len() > 0 {
            let fix_results = parallel(fix_jobs);
            let done = 0;
            for fr in fix_results {
                if fr != () && fr.success == true && fr.output.fixed == true { done += 1; }
            }
            log("Fix round " + rounds.to_string() + ": " + done.to_string() + "/" + fix_jobs.len().to_string() + " fix agents reported fixed=true");
        }
        phase("Verify");
        let opts = #{ label: "verify:round" + (rounds + 2).to_string(), capability_mode: "execute", output_schema: verify_schema };
        if v1 != () && v1.agent_id != () {
            opts.resume_from = v1.agent_id;
        }
        v1 = agent(verify_rerun_prompt, opts);
        rounds += 1;
    }
}

let final_pass = false;
let gates_text = "no gate data";
if v1 != () && v1.success == true && v1.output.gates != () {
    final_pass = true;
    gates_text = "";
    for g in v1.output.gates {
        let st = "FAIL";
        if g.pass == true { st = "PASS"; }
        gates_text += g.name + ": " + st + " — " + g.detail + "\n";
        if g.pass != true { final_pass = false; }
    }
}

let rep = "# ERP Feature Refactor Report\n\n";
rep += "Workflow: refactor-erp-features\n\n";
rep += "## Final verification gates\n\n" + gates_text + "\n";
rep += "## Per-job results (" + jobs.len().to_string() + " jobs)\n\n";
i = 0;
let total_tests = 0;
let job_failures = 0;
for r in results {
    let j = jobs[i];
    if r == () || r.success == false {
        rep += "- " + j.label + ": AGENT FAILED (no result)\n";
        job_failures += 1;
    } else {
        let created = 0;
        let modified = 0;
        let tests = 0;
        let run = 0;
        let failed_n = -1;
        if r.output.files_created != () { created = r.output.files_created.len(); }
        if r.output.files_modified != () { modified = r.output.files_modified.len(); }
        if r.output.tests_added != () { tests = r.output.tests_added.len(); }
        if r.output.tests_run != () { run = r.output.tests_run; total_tests += run; }
        if r.output.tests_failed != () { failed_n = r.output.tests_failed; }
        rep += "- " + j.label + ": created " + created.to_string() + ", modified " + modified.to_string() + ", test files " + tests.to_string() + ", tests run " + run.to_string();
        if failed_n != 0 { rep += ", tests failed " + failed_n.to_string(); job_failures += 1; }
        if r.output.summary != () { rep += " — " + r.output.summary; }
        rep += "\n";
    }
    i += 1;
}
rep += "\n## Summary\n\n";
rep += "Jobs: " + jobs.len().to_string() + ", jobs with failures: " + job_failures.to_string() + ", total hook tests run by refactor agents: " + total_tests.to_string() + "\n";
rep += "Final gates " + (if final_pass { "PASSED" } else { "FAILED" }) + " (see above).\n";
rep += "Verify agent summary: ";
if v1 != () && v1.success == true && v1.output.summary != () { rep += v1.output.summary; } else { rep += "verification agent failed"; }
rep += "\n";

let path = write_scratch_file("refactor-report.md", rep);
let status = "passed";
if final_pass == false { status = "needs-attention"; }
complete(#{ status: status, report_path: path, report: rep, final_gates_passed: final_pass, total_jobs: jobs.len(), jobs_with_failures: job_failures });
