import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Only the external recognition result is a fixture; upload and archive use real APIs. */
export function prepareRecognition(id, fields) {
  if (process.env.ERP_E2E_ISOLATED !== "1" || !process.env.ERP_E2E_CONFIG_PATH) {
    throw new Error("Recognition fixtures require an isolated E2E database");
  }
  const settings = JSON.parse(execFileSync("python3", ["-c",
    "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))",
    process.env.ERP_E2E_CONFIG_PATH], { encoding: "utf8" }));
  if (!/^erp_e2e_[0-9]{8}t[0-9]{6}_[0-9a-f]{12}_[0-9]+$/.test(settings.db_name)) {
    throw new Error("Refusing recognition fixture outside an owned E2E database");
  }
  const values = Object.fromEntries(Object.entries(fields).filter(([, value]) => value != null && value !== ""));
  const text = Object.values(values).join("\n");
  const extraction = {
    provider: "e2e-recognition-fixture", version: "1",
    fields: Object.fromEntries(Object.entries(values).map(([key, value]) => [key, { value, page: 1, quote: value }])),
    conflicts: [],
  };
  const script = `const target = db.getSiblingDB(${JSON.stringify(settings.db_name)});
    const task = target.contract_imports.findOne({id: ${JSON.stringify(id)}, status: 'ready'});
    if (!task) throw new Error('Expected a newly uploaded recognition task');
    const pages = Array.from({length: Number(task.source.page_count)}, (_, index) => ({
      number: index + 1, text: index === 0 ? ${JSON.stringify(text)} : '', blank: index > 0, readable: true
    }));
    const result = target.contract_imports.updateOne({_id: task._id, status: 'ready', version: task.version}, {
      $set: {status: 'review', stage: 'preparing_review', failure: null,
        ocr: {provider: 'e2e-recognition-fixture', version: '1', pages}, extraction: ${JSON.stringify(extraction)}},
      $inc: {version: NumberLong(1)}
    });
    if (result.modifiedCount !== 1) throw new Error('Recognition fixture CAS failed');`;
  try {
    execFileSync("mongosh", ["--norc", "--quiet", settings.uri, "--eval", script], { stdio: "pipe", timeout: 30_000 });
  } catch {
    throw new Error("Isolated recognition fixture preparation failed");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { id, fields } = JSON.parse(readFileSync(0, "utf8"));
  prepareRecognition(id, fields);
}
