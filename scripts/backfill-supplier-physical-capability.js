// 在 Navicat 的「工具 → 控制台」选择目标 ERP 库后，用 load() 执行本文件。
// 核对预览后，将 APPLY 改为 true，再在同一目标库执行。
{
    const APPLY = false;
    // 仅用于缺少维护人的供应商。填写实际 ERP 账号 ID 后再写入。
    const FALLBACK_OWNER_USER_ID = "";
    const ACTOR_ID = "migration:backfill-supplier-physical-capability";

    const supplierCollection = "supplier_accounts";
    const capabilityCollection = "supplier_capabilities";
    const revisionCollection = "supplier_capability_revisions";
    const database = db.getName();
    const preflightTimeoutMs = 30000;
    print(`开始检查供应商实物商品能力，数据库 ${database}，模式 ${APPLY ? "写入" : "预览"}`);

    const validOwner = owner => owner.length > 0 && Array.from(owner).length <= 128;
    const ownerOf = supplier => {
        const maintainer = typeof supplier.maintainer_user_id === "string" ? supplier.maintainer_user_id.trim() : "";
        return validOwner(maintainer) ? maintainer : FALLBACK_OWNER_USER_ID.trim();
    };
    const physicalOf = supplierId => ({ supplier_id: supplierId, capability_code: "physical" });
    const supplierFields = { _id: 0, id: 1, supplier_no: 1, maintainer_user_id: 1 };
    const pending = [];
    const missingOwner = [];
    let skippedExisting = 0;

    print("预检 1/2：读取供应商档案");
    const suppliers = db.getCollection(supplierCollection).find({ deleted_at: 0 }, supplierFields)
        .maxTimeMS(preflightTimeoutMs).toArray();
    print("预检 2/2：读取已有实物商品能力");
    const existing = db.getCollection(capabilityCollection).find(
        { capability_code: "physical" }, { _id: 0, supplier_id: 1 },
    ).maxTimeMS(preflightTimeoutMs).toArray();
    const suppliersWithPhysical = new Set(existing.map(capability => capability.supplier_id));
    for (const supplier of suppliers) {
        if (suppliersWithPhysical.has(supplier.id)) {
            skippedExisting++;
        } else if (validOwner(ownerOf(supplier))) {
            pending.push(supplier.id);
        } else {
            missingOwner.push({ id: supplier.id, supplier_no: supplier.supplier_no });
        }
    }

    printjson({ database, apply: APPLY, pending: pending.length, skippedExisting, missingOwner });

    if (APPLY && missingOwner.length > 0) {
        throw new Error("请为缺少有效负责人的供应商补齐维护人，或设置 FALLBACK_OWNER_USER_ID 后重跑");
    }

    if (APPLY && pending.length === 0) printjson({ database, inserted: 0 });

    if (APPLY && pending.length > 0) {
        const runAt = new Date();
        const businessDate = new Date(runAt.getTime() + 8 * 60 * 60 * 1000).toISOString().slice(0, 10);
        const timestamp = NumberLong(String(Math.floor(runAt.getTime() / 1000)));
        const base = {
            version: NumberLong("1"),
            created_at: timestamp,
            updated_at: timestamp,
            deleted_at: NumberLong("0"),
        };
        // 两种 Mongo Shell 的 UUID() 都生成 UUID v4，BSON 包装类型不同。
        const newId = () => {
            const uuid = UUID();
            const value = typeof uuid.toUUID === "function" ? uuid.toUUID().toHexString() : uuid.hex();
            return value.replace(/-/g, "").toLowerCase();
        };

        const session = db.getMongo().startSession({ readPreference: { mode: "primary" } });
        let inserted = 0;
        let processed = 0;
        try {
            if (["startTransaction", "commitTransaction", "abortTransaction"]
                .some(method => typeof session[method] !== "function")) {
                throw new Error("当前 Mongo Shell 不支持显式事务，请在 Navicat 控制台选择 mongosh");
            }
            print(`开始写入，待补录 ${pending.length} 家供应商（显式事务）`);
            const target = session.getDatabase(database);
            const targetSuppliers = target.getCollection(supplierCollection);
            const targetCapabilities = target.getCollection(capabilityCollection);
            const targetRevisions = target.getCollection(revisionCollection);

            for (const supplierId of pending) {
                session.startTransaction({ readConcern: { level: "snapshot" }, writeConcern: { w: "majority" } });
                let created = false;
                let committing = false;
                try {
                    const supplier = targetSuppliers.findOne({ id: supplierId, deleted_at: 0 }, supplierFields);
                    const identity = physicalOf(supplierId);
                    if (supplier && !targetCapabilities.findOne(identity, { _id: 1 })) {
                        const owner = ownerOf(supplier);
                        if (!validOwner(owner)) throw new Error(`供应商 ${supplier.supplier_no} 缺少有效负责人`);

                        const history = targetRevisions.find(identity, { revision_no: 1 })
                            .sort({ revision_no: -1 }).limit(1).toArray();
                        const revisionNo = history.length ? Number(history[0].revision_no) + 1 : 1;
                        if (!Number.isInteger(revisionNo) || revisionNo < 1 || revisionNo > 4294967295) {
                            throw new Error(`供应商 ${supplier.supplier_no} 的能力修订号无效`);
                        }

                        const fields = {
                            ...base,
                            ...identity,
                            status: "active",
                            owner_user_id: owner,
                            service_region: null,
                            fulfillment_note: null,
                            valid_from: businessDate,
                            valid_to: null,
                        };
                        const revisionId = newId();
                        const capabilityId = newId();
                        targetRevisions.insertOne({
                            ...fields,
                            id: revisionId,
                            revision_no: NumberLong(String(revisionNo)),
                        });
                        targetCapabilities.insertOne({
                            ...fields,
                            id: capabilityId,
                            current_revision_id: revisionId,
                            created_by: ACTOR_ID,
                            updated_by: ACTOR_ID,
                        });
                        created = true;
                    }

                    committing = true;
                    session.commitTransaction();
                } catch (error) {
                    if (!committing) {
                        try { session.abortTransaction(); } catch (_) { /* 保留写入错误。 */ }
                    }
                    printjson({ failedSupplierId: supplierId, inserted });
                    throw error;
                }
                if (created) inserted++;
                processed++;
                if (processed % 25 === 0 && processed < pending.length) {
                    print(`已处理 ${processed}/${pending.length} 家，已新增 ${inserted} 家`);
                }
            }
        } finally {
            session.endSession();
        }
        printjson({ database, inserted, skippedDuringApply: pending.length - inserted });
    }
}
