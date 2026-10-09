import { execFileSync } from "node:child_process"

/** 仅在当前隔离 shard 将本用例新建册转换为历史未设密码形态。 */
export function prepareLegacySelectionBook(
    bookletId: string,
    customerName: string,
): void {
    const configPath = process.env.ERP_E2E_CONFIG_PATH
    if (process.env.ERP_E2E_ISOLATED !== "1" || !configPath) {
        throw new Error("历史选品册 fixture 必须绑定本次隔离 E2E 配置")
    }
    if (
        !customerName.startsWith("E2E 选品客户 ") ||
        !/^[a-zA-Z0-9_-]+$/.test(bookletId)
    ) {
        throw new Error("历史选品册 fixture 只接受当前流程新建客户与选品册")
    }
    const settings = JSON.parse(
        execFileSync(
            "python3",
            [
                "-c",
                "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))",
                configPath,
            ],
            { encoding: "utf8", timeout: 10_000 },
        ),
    ) as { uri: string; db_name: string }
    if (
        !/^erp_e2e_[0-9]{8}t[0-9]{6}_[0-9a-f]{12}_[0-9]+$/.test(
            settings.db_name,
        )
    ) {
        throw new Error("拒绝在本次 runner 所有的临时数据库以外准备历史选品册")
    }
    const script = `const target = db.getSiblingDB(${JSON.stringify(settings.db_name)});
        const booklet = target.sales_selection_booklets.findOne({
            id: ${JSON.stringify(bookletId)}, customer_name: ${JSON.stringify(customerName)},
            status: "PUBLISHED", submit_mode: "BY_QUANTITY", proposal_id: null
        });
        if (!booklet || !booklet.access_password_hash) throw new Error("Expected the new published selection booklet");
        const result = target.sales_selection_booklets.updateOne({_id: booklet._id, version: booklet.version}, {
            $unset: {access_password_hash: ""}, $inc: {version: NumberLong(1)}
        });
        if (result.modifiedCount !== 1) throw new Error("Legacy selection fixture CAS failed");
        if (target.sales_selection_booklets.findOne({_id: booklet._id}).access_password_hash !== undefined)
            throw new Error("Legacy password field must be absent");`
    try {
        execFileSync(
            "mongosh",
            ["--norc", "--quiet", settings.uri, "--eval", script],
            {
                stdio: "pipe",
                timeout: 30_000,
            },
        )
    } catch {
        throw new Error("本次隔离 shard 的历史选品册 fixture 准备失败")
    }
}
