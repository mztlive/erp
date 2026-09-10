/* global db, EJSON, Long, print */
// 默认只读检查；在停写窗口使用 ERP_COMPANY_MIGRATION_MODE=apply/rollback。
// ERP_COMPANY_MAPPING_FILE 为明确批准的既有 Party ID 与别名映射，不按名称推断角色。
const fs = require('node:fs');
const mode = process.env.ERP_COMPANY_MIGRATION_MODE || 'check';
const backupName = 'migration_20260910_supplier_companies';
const actor = process.env.ERP_MIGRATION_ACTOR;
if (!['check', 'apply', 'rollback'].includes(mode)) throw new Error('迁移模式无效');
if (mode !== 'check' && !actor) throw new Error('写入必须设置 ERP_MIGRATION_ACTOR');
const normalize = value => value.replace(/\s/gu, '').replaceAll('（', '(').replaceAll('）', ')').toLowerCase();
const same = (a, b) => EJSON.stringify(a, {relaxed:false}) === EJSON.stringify(b, {relaxed:false});
const mappings = process.env.ERP_COMPANY_MAPPING_FILE
  ? JSON.parse(fs.readFileSync(process.env.ERP_COMPANY_MAPPING_FILE, 'utf8')) : [];
if (!Array.isArray(mappings)) throw new Error('映射文件必须为数组');
const plans = [];
const owner = new Map();
for (const company of db.parties.find({'company_profile.names':{$type:'array'}})) {
  for (const name of company.company_profile.names) {
    if (owner.has(name) && owner.get(name) !== company.id) throw new Error('公司名称或别名存在重复');
    owner.set(name, company.id);
  }
}
for (const mapping of mappings) {
  if (typeof mapping.party_id !== 'string' || !mapping.party_id || !Array.isArray(mapping.aliases)) throw new Error('每项必须提供 party_id 和 aliases');
  if (plans.some(plan => plan.id === mapping.party_id)) throw new Error('映射重复登记同一主体');
  const party = db.parties.findOne({id:mapping.party_id, deleted_at:0});
  if (!party || party.party_kind !== 'enterprise') throw new Error('映射主体不存在或不是企业');
  const revision = db.party_revisions.findOne({id:party.current_revision_id, party_id:party.id, deleted_at:0});
  if (!revision || !revision.legal_name?.trim()) throw new Error('主体缺少当前法定名称');
  const legal_name = revision.legal_name.trim();
  const short_name = revision.short_name?.trim() || null;
  if (mapping.aliases.some(alias => typeof alias !== 'string')) throw new Error('别名必须是文本');
  const aliases = [...new Set(mapping.aliases.map(alias => alias.trim()).filter(Boolean))].sort();
  if ([...legal_name].length > 256 || mapping.aliases.length > 32 || [short_name, ...aliases].filter(Boolean).some(name => [...name].length > 128)) throw new Error('名称或别名超过限制');
  const names = [...new Set([legal_name, short_name, ...aliases].filter(Boolean).map(normalize))].sort();
  const profile = {legal_name, short_name, aliases, names};
  if (party.company_profile && !same(party.company_profile, profile)) throw new Error('已有公司资料不同，应通过维护页面修改');
  for (const name of names) {
    if (owner.has(name) && owner.get(name) !== party.id) throw new Error('映射名称或别名与其他公司冲突');
    owner.set(name, party.id);
  }
  if (!party.company_profile) plans.push({id:party.id, before:party, profile});
}
const companyIds = new Set([...owner.values()]);
const missing = new Set();
for (const supplier of db.supplier_accounts.find({deleted_at:0})) {
  if (!supplier.current_commercial_profile_revision_id) continue;
  const profile = db.supplier_commercial_profile_revisions.findOne({id:supplier.current_commercial_profile_revision_id});
  if (!profile) throw new Error('现有供应商缺少当前商务版本');
  for (const id of [profile.signing_entity_party_id, profile.payment_entity_party_id]) {
    if (id && !companyIds.has(id)) missing.add(id);
  }
}
if (mode !== 'rollback') {
  print(JSON.stringify({mode, companyRegistrations:plans.length, unmappedCompanyPartyIds:[...missing]}));
  if (missing.size) throw new Error('现有签约或付款主体尚未完成明确的公司角色映射；不得上线新写入');
}
const uniqueOptions = {name:'uk_company_names', unique:true, partialFilterExpression:{'company_profile.names':{$type:'array'}}};
const listOptions = {name:'idx_company_status', partialFilterExpression:{company_profile:{$type:'object'}}};
if (mode === 'apply') {
  db.parties.createIndex({'company_profile.names':1}, uniqueOptions);
  db.parties.createIndex({status:1, party_no:1, id:1}, listOptions);
  db.supplier_qualification_capabilities.createIndex({capability_id:1, qualification_id:1}, {name:'idx_supplier_capability_qualifications'});
  if (!db.getCollectionNames().includes(backupName)) db.createCollection(backupName);
  const session = db.getMongo().startSession();
  try {
    session.withTransaction(() => {
      const target = session.getDatabase(db.getName());
      for (const plan of plans) {
        const afterVersion = Long.fromString(String(plan.before.version)).add(Long.ONE);
        target.getCollection(backupName).insertOne({_id:plan.id, before:plan.before, afterVersion, profile:plan.profile, actor, migratedAt:new Date()});
        const result = target.parties.updateOne({id:plan.id, version:plan.before.version, company_profile:null},
          {$set:{company_profile:plan.profile, updated_by:actor, updated_at:Long.fromNumber(Math.floor(Date.now() / 1000)), version:afterVersion}});
        if (result.matchedCount !== 1) throw new Error('主体已变化，迁移整体撤销');
      }
    });
  } finally { session.endSession(); }
  print(JSON.stringify({registered:plans.length, indexesCreated:true}));
}
if (mode === 'rollback') {
  if (db.supplier_qualifications.findOne({valid_from:null}) || db.supplier_qualification_revisions.findOne({valid_from:null})) {
    throw new Error('已有未知起始日期的合同；旧程序不能读取，禁止自动回滚');
  }
  if (db.supplier_commercial_profile_revisions.findOne({$or:[{settlement_mode:{$in:['weekly','monthly','quarterly','half_yearly','yearly']}},{invoice_tax_rate:null}]})) {
    throw new Error('已有旧程序无法读取的新商务版本；禁止自动回滚，须使用发布前备份执行完整恢复');
  }
  const backups = db.getCollection(backupName).find({rolledBackAt:{$exists:false}}).toArray();
  const backedIds = new Set(backups.map(item => item._id));
  for (const company of db.parties.find({company_profile:{$type:'object'}})) {
    if (!backedIds.has(company.id)) throw new Error('上线后新增公司，不允许自动撤销公司角色');
  }
  const session = db.getMongo().startSession();
  try {
    session.withTransaction(() => {
      const target = session.getDatabase(db.getName());
      for (const backup of backups) {
        const current = target.parties.findOne({id:backup._id, version:backup.afterVersion});
        if (!current || !same(current.company_profile, backup.profile)) throw new Error('迁移后公司资料已修改，禁止覆盖');
        const before = backup.before;
        const result = target.parties.updateOne({id:backup._id, version:backup.afterVersion},
          {$unset:{company_profile:''}, $set:{updated_by:actor, updated_at:Long.fromNumber(Math.floor(Date.now() / 1000))}, $inc:{version:Long.ONE}});
        if (result.matchedCount !== 1) throw new Error('回滚版本冲突');
        target.getCollection(backupName).updateOne({_id:before.id}, {$set:{rolledBackAt:new Date(), rollbackActor:actor}});
      }
    });
  } finally { session.endSession(); }
  for (const name of ['uk_company_names','idx_company_status']) {
    if (db.parties.getIndexes().some(index => index.name === name)) db.parties.dropIndex(name);
  }
  if (db.supplier_qualification_capabilities.getIndexes().some(index => index.name === 'idx_supplier_capability_qualifications')) {
    db.supplier_qualification_capabilities.dropIndex('idx_supplier_capability_qualifications');
  }
  print(JSON.stringify({rolledBack:backups.length}));
}
