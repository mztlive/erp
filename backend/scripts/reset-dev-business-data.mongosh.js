/* global EJSON, Mongo, process, quit */
/* eslint-disable no-console */

// 由 reset-dev-business-data.sh 调用。连接串只从进程环境读取，禁止输出。
// 本文件默认只执行 count/distinct/aggregate；ERP_RESET_EXECUTE=1 才进入写分支。

const DROP_GROUPS = [
    {
        name: "安全暂停叶子证据",
        collections: ["system_safety_pause_operations"],
    },
    {
        name: "审批与待办（不兼容旧 schema，必须 drop）",
        collections: [
            "work_items",
            "approval_step_instances",
            "approval_instances",
            "approval_step_definitions",
            "approval_definitions",
        ],
    },
    {
        name: "通用集成与批处理运行数据",
        collections: [
            "reconciliation_difference_resolutions",
            "reconciliation_differences",
            "integration_error_tasks",
            "supplier_api_health_check_runs",
            "supplier_api_connection_command_receipts",
            "background_job_items",
            "background_jobs",
            "bulk_selection_items",
            "bulk_selection_snapshots",
            "legacy_import_confirmations",
            "legacy_import_rows",
            "legacy_import_batches",
        ],
    },
    {
        name: "业务单据注册表",
        collections: [
            "document_attachments",
            "workflow_actions",
            "document_participants",
            "document_relations",
            "business_documents",
        ],
    },
    {
        name: "商城售后、回填与同步",
        collections: [
            "mall_snapshot_reapply_operations",
            "mall_balance_restoration_allocations",
            "mall_balance_restorations",
            "mall_refund_allocations",
            "mall_refund_lines",
            "mall_refunds",
            "mall_consumption_backfill_items",
            "mall_consumption_backfill_jobs",
            "mall_sales_reconciliation_items",
            "mall_sales_reconciliation_jobs",
            "master_mapping_tasks",
            "mall_sales_order_snapshots",
            "mall_sales_sync_cursors",
            "mall_sales_sync_jobs",
        ],
    },
    {
        name: "销售投影与商城订单事实",
        collections: [
            "sales_order_projection_deliveries",
            "sales_order_projection_revisions",
            "sales_order_projections",
            "mall_consumption_cost_assessments",
            "mall_item_funding_allocations",
            "mall_consumption_entries",
            "mall_payment_sources",
            "mall_order_items",
            "mall_orders",
            "mall_order_completion_facts",
            "mall_order_cancel_facts",
            "mall_order_facts",
        ],
    },
    {
        name: "卡实例与余额事实",
        collections: [
            "mall_card_instance_corrections",
            "mall_balance_snapshots",
            "mall_card_instances",
            "mall_consumption_cutovers",
        ],
    },
    {
        name: "供应商履约与结算运行数据（保留供应商主数据）",
        collections: [
            "supplier_settlement_difference_evidence",
            "supplier_settlement_source_evidence",
            "supplier_settlement_differences",
            "supplier_settlement_items",
            "supplier_settlement_statements",
            "supplier_refund_allocations",
            "supplier_refund_facts",
            "supplier_order_action_lines",
            "supplier_order_actions",
            "supplier_order_status_histories",
            "supplier_fulfillment_items",
            "supplier_fulfillment_orders",
            "mall_after_sales_request_lines",
            "mall_after_sales_requests",
        ],
    },
    {
        name: "商品发布运行链（保留商品与供给主数据）",
        collections: [
            "product_publication_deliveries",
            "product_publication_revision_media",
            "product_publication_revisions",
            "product_publications",
        ],
    },
    {
        name: "已消费的入站消息根事实",
        collections: ["inbox_messages"],
    },
    {
        name: "退货、应收、应付与成本",
        collections: [
            "payment_reversals",
            "receipt_reversals",
            "supplier_refunds",
            "customer_refunds",
            "purchase_return_lines",
            "purchase_return_orders",
            "sales_return_lines",
            "sales_return_cases",
            "sales_invoice_allocations",
            "receipt_allocations",
            "receivable_entry_offsets",
            "receivable_funds_reviews",
            "receivable_entries",
            "customer_receipts",
            "invoices",
            "receivable_accounts",
            "purchase_invoice_allocations",
            "payment_allocations",
            "payable_entry_offsets",
            "payable_entries",
            "supplier_payments",
            "payable_accounts",
            "cost_allocations",
            "cost_entries",
        ],
    },
    {
        name: "交付与库存账本（整体重置，禁止只删预留）",
        collections: [
            "acceptance_fulfillment_allocations",
            "customer_acceptance_lines",
            "customer_acceptances",
            "service_fulfillments",
            "electronic_deliveries",
            "delivery_lines",
            "deliveries",
            "stock_reservation_entries",
            "stock_reservations",
            "stock_balances",
            "stock_movements",
            "purchase_receipt_lines",
            "purchase_receipts",
            "stock_adjustment_lines",
            "stock_adjustments",
        ],
    },
    {
        name: "采购单运行链",
        collections: [
            "purchase_change_submission_lines",
            "purchase_change_submissions",
            "purchase_change_orders",
            "purchase_line_sales_allocations",
            "purchase_order_revision_lines",
            "purchase_order_revisions",
            "purchase_order_submission_lines",
            "purchase_order_submissions",
            "purchase_orders",
        ],
    },
    {
        name: "销售复核、提交、草稿、变更与版本",
        collections: [
            "sales_change_reviews",
            "sales_change_submission_lines",
            "sales_change_submissions",
            "low_margin_manager_confirmations",
            "procurement_confirmation_lines",
            "procurement_confirmations",
            "sales_order_reviews",
            "sales_order_submission_lines",
            "sales_order_submissions",
            "sales_order_working_copy_lines",
            "sales_order_working_copies",
            "sales_change_orders",
            "sales_order_goods_service_line_revisions",
            "sales_order_voucher_line_revisions",
            "sales_order_revision_lines",
            "sales_order_revisions",
            "sales_order_lines",
            "sales_orders",
        ],
    },
    {
        name: "合同与客户角色",
        collections: [
            "contract_revisions",
            "contracts",
            "customer_assignments",
            "customer_profile_commands",
            "customer_accounts",
        ],
    },
];

const PARTY_CHILD_COLLECTIONS = [
    "party_bank_accounts",
    "party_tax_profiles",
    "party_addresses",
    "party_contacts",
    "party_revisions",
];

const RESET_OBJECT_TYPES = ["customer", "contract", "sales_order"];
const BATCH_SIZE = 500;

const BEFORE_RELATIONS = [
    ["sales_order.customer", "sales_orders", "customer_id", "customer_accounts", "id"],
    ["sales_order.contract", "sales_orders", "contract_id", "contracts", "id"],
    ["contract.customer", "contracts", "customer_id", "customer_accounts", "id"],
    ["contract_revision.contract", "contract_revisions", "contract_id", "contracts", "id"],
    ["customer.party", "customer_accounts", "party_id", "parties", "id"],
    ["supplier.party", "supplier_accounts", "party_id", "parties", "id"],
    [
        "supplier_commercial.signing_party",
        "supplier_commercial_profile_revisions",
        "signing_entity_party_id",
        "parties",
        "id",
    ],
    [
        "supplier_commercial.payment_party",
        "supplier_commercial_profile_revisions",
        "payment_entity_party_id",
        "parties",
        "id",
    ],
    [
        "approval_step_definition.definition",
        "approval_step_definitions",
        "approval_definition_id",
        "approval_definitions",
        "id",
    ],
    [
        "approval_step_instance.instance",
        "approval_step_instances",
        "approval_instance_id",
        "approval_instances",
        "id",
    ],
    [
        "approval_instance.current_step",
        "approval_instances",
        "current_step_instance_id",
        "approval_step_instances",
        "id",
    ],
    [
        "work_item.approval_step",
        "work_items",
        "approval_step_instance_id",
        "approval_step_instances",
        "id",
    ],
    [
        "external_target.map",
        "external_identity_targets",
        "external_identity_map_id",
        "external_identity_maps",
        "id",
    ],
];

const PRESERVED_RELATION_NAMES = new Set([
    "supplier.party",
    "supplier_commercial.signing_party",
    "supplier_commercial.payment_party",
    "external_target.map",
]);

function line(message = "") {
    console.log(message);
}

function bsonKey(value) {
    return EJSON.stringify(value, { relaxed: false });
}

function uniqueValues(values) {
    const unique = new Map();
    for (const value of values) {
        if (value !== null && value !== undefined && value !== "") {
            unique.set(bsonKey(value), value);
        }
    }
    return [...unique.values()];
}

function chunks(values) {
    const result = [];
    for (let offset = 0; offset < values.length; offset += BATCH_SIZE) {
        result.push(values.slice(offset, offset + BATCH_SIZE));
    }
    return result;
}

async function run() {
    const uri = process.env.ERP_RESET_MONGO_URI;
    const dbName = process.env.ERP_RESET_DB_NAME;
    const execute = process.env.ERP_RESET_EXECUTE === "1";
    const confirmedDb = process.env.ERP_RESET_CONFIRMED_DB || "";

    if (!uri || !dbName) {
        throw new Error("missing_configuration");
    }
    if (execute && confirmedDb !== dbName) {
        throw new Error("execution_not_confirmed");
    }

    let connection;
    let targetDb;
    try {
        connection = new Mongo(uri);
        targetDb = connection.getDB(dbName);
        const ping = await targetDb.runCommand({ ping: 1 });
        if (!ping || ping.ok !== 1) {
            throw new Error("ping_failed");
        }
    } catch (_error) {
        throw new Error("connection_failed");
    }

    const existing = new Set(await targetDb.getCollectionNames());

    async function countDocuments(collectionName, filter = {}) {
        if (!existing.has(collectionName)) {
            return 0;
        }
        return await targetDb.getCollection(collectionName).countDocuments(filter);
    }

    async function distinct(collectionName, field, filter = {}) {
        if (!existing.has(collectionName)) {
            return [];
        }
        return uniqueValues(await targetDb.getCollection(collectionName).distinct(field, filter));
    }

    async function distinctReferenced(collectionName, fields, candidates) {
        if (!existing.has(collectionName) || candidates.length === 0) {
            return [];
        }
        const found = [];
        for (const batch of chunks(candidates)) {
            for (const field of fields) {
                found.push(...(await distinct(collectionName, field, { [field]: { $in: batch } })));
            }
        }
        return uniqueValues(found);
    }

    async function countByValues(collectionName, field, values, baseFilter = {}) {
        let count = 0;
        for (const batch of chunks(values)) {
            count += await countDocuments(collectionName, {
                ...baseFilter,
                [field]: { $in: batch },
            });
        }
        return count;
    }

    async function deleteByValues(collectionName, field, values, baseFilter = {}) {
        if (!existing.has(collectionName) || values.length === 0) {
            return 0;
        }
        let deleted = 0;
        for (const batch of chunks(values)) {
            const result = await targetDb.getCollection(collectionName).deleteMany({
                ...baseFilter,
                [field]: { $in: batch },
            });
            deleted += result.deletedCount;
        }
        return deleted;
    }

    async function danglingCount(source, localField, foreign, foreignField) {
        if (!existing.has(source)) {
            return 0;
        }
        const rows = await targetDb
            .getCollection(source)
            .aggregate(
                [
                    { $match: { [localField]: { $exists: true, $nin: [null, ""] } } },
                    {
                        $lookup: {
                            from: foreign,
                            localField,
                            foreignField,
                            as: "__reset_target",
                        },
                    },
                    { $match: { "__reset_target.0": { $exists: false } } },
                    { $count: "count" },
                ],
                { allowDiskUse: true },
            )
            .toArray();
        return rows.length === 0 ? 0 : Number(rows[0].count);
    }

    async function verifyRelations(title) {
        line(title);
        const result = new Map();
        for (const [name, source, localField, foreign, foreignField] of BEFORE_RELATIONS) {
            const count = await danglingCount(source, localField, foreign, foreignField);
            result.set(name, count);
            line(`  ${name}: ${count}`);
        }
        return result;
    }

    const customerPartyIds = await distinct("customer_accounts", "party_id");
    const resetPartyIds = uniqueValues([
        ...customerPartyIds,
        ...(await distinct("contracts", "settlement_party_id")),
        ...(await distinct("contract_revisions", "settlement_party_id")),
        ...(await distinct("sales_orders", "settlement_party_id")),
        ...(await distinct("sales_order_working_copies", "settlement_party_id")),
        ...(await distinct("sales_order_submissions", "settlement_party_id")),
        ...(await distinct("sales_change_submissions", "settlement_party_id")),
        ...(await distinct("receivable_accounts", "counterparty_party_id")),
        ...(await distinct("customer_receipts", "counterparty_party_id")),
        ...(await distinct("invoices", "party_id")),
    ]);
    const protectedPartyIds = uniqueValues([
        ...(await distinctReferenced("supplier_accounts", ["party_id"], resetPartyIds)),
        ...(await distinctReferenced(
            "supplier_commercial_profile_revisions",
            ["signing_entity_party_id", "payment_entity_party_id"],
            resetPartyIds,
        )),
    ]);
    const protectedKeys = new Set(protectedPartyIds.map(bsonKey));
    const deletablePartyIds = resetPartyIds.filter((value) => !protectedKeys.has(bsonKey(value)));

    const resetMapIds = await distinct("external_identity_maps", "id", {
        object_type: { $in: RESET_OBJECT_TYPES },
    });
    const partyTargetMapIds = [];
    for (const batch of chunks(deletablePartyIds)) {
        partyTargetMapIds.push(
            ...(await distinct("external_identity_targets", "external_identity_map_id", {
                internal_object_type: "party",
                internal_object_id: { $in: batch },
            })),
        );
    }
    const affectedPartyMapIds = uniqueValues(partyTargetMapIds);
    const retainedFileIds = uniqueValues([
        ...(await distinct("contract_revisions", "contract_pdf_file_id")),
        ...(await distinct("document_attachments", "file_asset_id")),
    ]);

    line();
    line("== 执行前范围 ==");
    let plannedDocuments = 0;
    for (const group of DROP_GROUPS) {
        let groupDocuments = 0;
        const nonEmpty = [];
        for (const collectionName of group.collections) {
            const count = await countDocuments(collectionName);
            groupDocuments += count;
            if (count > 0) {
                nonEmpty.push(`${collectionName}=${count}`);
            }
        }
        plannedDocuments += groupDocuments;
        line(`- ${group.name}: ${groupDocuments}`);
        for (const entry of nonEmpty) {
            line(`    ${entry}`);
        }
    }

    const sourceTargetsByType = await countDocuments("external_identity_targets", {
        internal_object_type: { $in: RESET_OBJECT_TYPES },
    });
    const sourceMaps = await countDocuments("external_identity_maps", {
        object_type: { $in: RESET_OBJECT_TYPES },
    });
    let sourceTargetsByMap = 0;
    for (const batch of chunks(resetMapIds)) {
        sourceTargetsByMap += await countDocuments("external_identity_targets", {
            external_identity_map_id: { $in: batch },
        });
    }
    const deletablePartyTargets = await countByValues(
        "external_identity_targets",
        "internal_object_id",
        deletablePartyIds,
        { internal_object_type: "party" },
    );
    const connectionHealthCaches = await countDocuments("supplier_api_connections", {
        $or: [
            { last_health_at: { $ne: null } },
            { last_health_result: { $ne: null } },
            { last_healthy_technical_config_version: { $ne: null } },
        ],
    });

    line(`- customer/contract/sales external_identity_maps: ${sourceMaps}`);
    line(`- customer/contract/sales external_identity_targets（按类型）: ${sourceTargetsByType}`);
    line(`- customer/contract/sales external_identity_targets（按映射）: ${sourceTargetsByMap}`);
    line(`- 将删除的客户及结算链专属 Party external_identity_targets: ${deletablePartyTargets}`);
    line(`- 将失效的供应商 API 连接技术健康缓存: ${connectionHealthCaches}`);
    line(`- 客户账户 Party: ${customerPartyIds.length}`);
    line(`- 客户/合同/销售结算链 Party: ${resetPartyIds.length}`);
    line(`- 将保留的供应商共享 Party: ${protectedPartyIds.length}`);
    line(`- 将删除的客户及结算链专属 Party: ${deletablePartyIds.length}`);
    for (const collectionName of PARTY_CHILD_COLLECTIONS) {
        line(`    ${collectionName}: ${await countByValues(collectionName, "party_id", deletablePartyIds)}`);
    }
    line(`    parties: ${await countByValues("parties", "id", deletablePartyIds)}`);
    line(`- 保留的文件资产候选: ${retainedFileIds.length}（不删除 file_assets，不操作对象存储）`);
    line(`- drop 集合内文档合计: ${plannedDocuments}`);

    const beforeDangling = await verifyRelations("== 执行前悬挂引用 ==");

    if (!execute) {
        line();
        line("预览完成：未执行任何写入。核对范围后使用 --execute --confirm-db <db_name>。 ");
        return;
    }

    line();
    line("== 执行清理 ==");

    // 健康检查运行事实将被清空，保留的连接不得继续携带旧的健康放行缓存。
    // 同时停用连接并递增乐观锁版本，要求重启后重新健康检查和显式启用。
    let invalidatedConnectionHealthCaches = 0;
    if (existing.has("supplier_api_connections")) {
        const result = await targetDb.getCollection("supplier_api_connections").updateMany(
            {
                $or: [
                    { last_health_at: { $ne: null } },
                    { last_health_result: { $ne: null } },
                    { last_healthy_technical_config_version: { $ne: null } },
                ],
            },
            {
                $set: { status: "disabled" },
                $unset: {
                    last_health_at: "",
                    last_health_result: "",
                    last_healthy_technical_config_version: "",
                },
                $inc: { version: 1 },
            },
        );
        invalidatedConnectionHealthCaches = result.modifiedCount;
    }
    line(`- 供应商 API 连接技术健康缓存已失效并停用: ${invalidatedConnectionHealthCaches}`);

    // Party 候选依赖即将删除的客户、合同和销售来源事实。必须在 drop
    // 这些来源集合前完成 Party 链清理；若进程在此后中断，来源集合仍在，
    // 下次重跑可重建同一候选集并幂等继续，不会因来源先被 drop 而永久漏删。
    let deletedPartyTargets = 0;
    if (existing.has("external_identity_targets")) {
        deletedPartyTargets = await deleteByValues(
            "external_identity_targets",
            "internal_object_id",
            deletablePartyIds,
            { internal_object_type: "party" },
        );
    }
    const retainedAffectedPartyMapIds = await distinctReferenced(
        "external_identity_targets",
        ["external_identity_map_id"],
        affectedPartyMapIds,
    );
    const retainedAffectedPartyMapKeys = new Set(retainedAffectedPartyMapIds.map(bsonKey));
    const orphanedPartyMapIds = affectedPartyMapIds.filter(
        (value) => !retainedAffectedPartyMapKeys.has(bsonKey(value)),
    );
    const deletedPartyMaps = await deleteByValues(
        "external_identity_maps",
        "id",
        orphanedPartyMapIds,
    );

    let deletedPartyChildren = 0;
    for (const collectionName of PARTY_CHILD_COLLECTIONS) {
        deletedPartyChildren += await deleteByValues(collectionName, "party_id", deletablePartyIds);
    }
    const deletedParties = await deleteByValues("parties", "id", deletablePartyIds);
    line(`- 客户及结算链专属 Party targets 已删除: ${deletedPartyTargets}`);
    line(`- 客户及结算链专属 Party 孤立 maps 已删除: ${deletedPartyMaps}`);
    line(`- 客户及结算链专属 Party 子记录已删除: ${deletedPartyChildren}`);
    line(`- 客户及结算链专属 Party 根记录已删除: ${deletedParties}`);
    line(`- 供应商共享 Party 已保护: ${protectedPartyIds.length}`);

    for (const group of DROP_GROUPS) {
        let dropped = 0;
        for (const collectionName of group.collections) {
            if (!existing.has(collectionName)) {
                continue;
            }
            await targetDb.getCollection(collectionName).drop();
            existing.delete(collectionName);
            dropped += 1;
        }
        line(`- ${group.name}: drop ${dropped} 个集合`);
    }

    let deletedTargets = deletedPartyTargets;
    if (existing.has("external_identity_targets")) {
        const byType = await targetDb.getCollection("external_identity_targets").deleteMany({
            internal_object_type: { $in: RESET_OBJECT_TYPES },
        });
        deletedTargets += byType.deletedCount;
        deletedTargets += await deleteByValues(
            "external_identity_targets",
            "external_identity_map_id",
            resetMapIds,
        );
    }
    let deletedMaps = deletedPartyMaps + (existing.has("external_identity_maps")
        ? (
              await targetDb.getCollection("external_identity_maps").deleteMany({
                  object_type: { $in: RESET_OBJECT_TYPES },
              })
          ).deletedCount
        : 0);
    line(`- external_identity_targets 已删除: ${deletedTargets}`);
    line(`- external_identity_maps 已删除: ${deletedMaps}`);

    line();
    line("== 执行后验证 ==");
    let remainingResetDocuments = 0;
    let remainingResetCollections = 0;
    const currentCollections = new Set(await targetDb.getCollectionNames());
    for (const group of DROP_GROUPS) {
        for (const collectionName of group.collections) {
            if (currentCollections.has(collectionName)) {
                remainingResetCollections += 1;
                remainingResetDocuments += await targetDb.getCollection(collectionName).countDocuments({});
            }
        }
    }
    const remainingSourceMaps = await countDocuments("external_identity_maps", {
        object_type: { $in: RESET_OBJECT_TYPES },
    });
    const remainingSourceTargets = await countDocuments("external_identity_targets", {
        internal_object_type: { $in: RESET_OBJECT_TYPES },
    });
    let remainingTargetsByDeletedMap = 0;
    for (const batch of chunks(resetMapIds)) {
        remainingTargetsByDeletedMap += await countDocuments("external_identity_targets", {
            external_identity_map_id: { $in: batch },
        });
    }
    const remainingDeletablePartyTargets = await countByValues(
        "external_identity_targets",
        "internal_object_id",
        deletablePartyIds,
        { internal_object_type: "party" },
    );
    let remainingPartyRows = await countByValues("parties", "id", deletablePartyIds);
    for (const collectionName of PARTY_CHILD_COLLECTIONS) {
        remainingPartyRows += await countByValues(collectionName, "party_id", deletablePartyIds);
    }
    const remainingConnectionHealthCaches = await countDocuments("supplier_api_connections", {
        $or: [
            { last_health_at: { $ne: null } },
            { last_health_result: { $ne: null } },
            { last_healthy_technical_config_version: { $ne: null } },
        ],
    });

    line(`- 残留 reset 集合: ${remainingResetCollections}`);
    line(`- 残留 reset 文档: ${remainingResetDocuments}`);
    line(`- 残留 customer/contract/sales external maps: ${remainingSourceMaps}`);
    line(`- 残留 customer/contract/sales external targets: ${remainingSourceTargets}`);
    line(`- 残留指向已删 customer/contract/sales map 的 targets: ${remainingTargetsByDeletedMap}`);
    line(`- 残留指向已删客户及结算链专属 Party 的 targets: ${remainingDeletablePartyTargets}`);
    line(`- 残留客户及结算链专属 Party/子记录: ${remainingPartyRows}`);
    line(`- 残留供应商 API 连接技术健康缓存: ${remainingConnectionHealthCaches}`);

    const afterDangling = await verifyRelations("== 执行后悬挂引用 ==");
    let danglingPostconditionFailed = false;
    for (const [name, count] of afterDangling.entries()) {
        if (PRESERVED_RELATION_NAMES.has(name)) {
            if (count > (beforeDangling.get(name) || 0)) {
                danglingPostconditionFailed = true;
            }
        } else if (count !== 0) {
            danglingPostconditionFailed = true;
        }
    }

    const failed =
        remainingResetCollections !== 0 ||
        remainingResetDocuments !== 0 ||
        remainingSourceMaps !== 0 ||
        remainingSourceTargets !== 0 ||
        remainingTargetsByDeletedMap !== 0 ||
        remainingDeletablePartyTargets !== 0 ||
        remainingPartyRows !== 0 ||
        remainingConnectionHealthCaches !== 0 ||
        danglingPostconditionFailed;

    if (failed) {
        throw new Error("postcondition_failed");
    }

    line();
    line("清理完成：重置后置条件通过。必须重启应用以重建索引和审批定义，再执行应用级验收。");
    line("保留项：账号/RBAC、供应商主数据、商品/仓库主数据、source_systems、file_assets、对象存储、审计记录、编号计数器。");
}

run()
    .then(() => quit(0))
    .catch((error) => {
        const code = error && error.message ? error.message : "unknown";
        if (code === "connection_failed") {
            console.error("错误: 无法连接或 ping 目标 MongoDB（URI 与凭据已隐藏）。");
        } else if (code === "missing_configuration") {
            console.error("错误: 缺少受控入口提供的数据库配置。");
        } else if (code === "execution_not_confirmed") {
            console.error("错误: mongosh 写分支缺少与目标数据库一致的二次确认。");
        } else if (code === "postcondition_failed") {
            console.error("错误: 清理后置条件未通过；请保持写入停止并按上方残留计数处理后重跑。");
        } else {
            console.error("错误: MongoDB 重置失败（详细驱动错误已隐藏，避免泄露连接信息）。可在停写状态下幂等重跑。");
        }
        quit(1);
    });
