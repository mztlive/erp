#!/usr/bin/env node
/**
 * S2 岗位账号业务验收。只在专用本地验收服务执行；账号取开发种子目录。
 * 前置：种子主数据、已发布审批定义，以及 S2_BALANCE_ID 指定的库存余额。
 * 本脚本创建组织、配置范围、创建并处理真实库存审批；不得指向共享开发库或生产库。
 */
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { ACCOUNTS, ADMIN, API_BASE, call, login, listAdmins } from './dev-seed-lib.mjs';

assert.ok(['127.0.0.1', 'localhost'].includes(new URL(API_BASE).hostname), '验收只允许本地专用服务');
assert.ok(process.env.S2_BALANCE_ID, '必须显式指定验收库存余额 S2_BALANCE_ID');
const run = randomUUID().replaceAll('-', '');
const tokens = {};
for (const key of ['admin', 'warehouse', 'finance', 'management', 'sales']) {
  const account = key === 'admin' ? ADMIN : ACCOUNTS[key];
  tokens[key] = await login(account.account, account.password);
}
const admins = await listAdmins(tokens.admin);
const person = key => admins.find(a => a.account === ACCOUNTS[key].account)?.id;
for (const key of ['warehouse','finance','management','sales']) assert.ok(person(key), `种子账号缺失 ${key}`);
async function change(change, retries = 3) {
  for (let attempt = 0; ; attempt += 1) {
    const state = await call('GET','/admin/org-units',{token:tokens.admin});
    try {
      return await call('POST','/admin/org-units/change',{token:tokens.admin,body:{expected_version:state.version,idempotency_key:randomUUID(),reason:'S2 专用环境业务验收',change}});
    } catch (error) {
      // 同秒重复调岗按合同拒绝，等待进入下一秒后重试；版本冲突直接重试。
      if (attempt < retries && (String(error.message).includes('关系刚生效') || String(error.message).includes('组织范围已变化'))) {
        await new Promise(resolve => setTimeout(resolve, 1100));
        continue;
      }
      throw error;
    }
  }
}
async function department(name) {
  const receipt = await change({operation:'create_unit',name,parent_id:null,kind:'department'});
  return receipt.after.units.find(u=>u.name===name).id;
}
async function scope(subject_type,subject_id,resource,actions,target_dimension,targets) {
  const page = await call('GET',`/admin/data-scopes?subject_type=${subject_type}&subject_id=${encodeURIComponent(subject_id)}&resource=${resource}&page=1&page_size=100`,{token:tokens.admin});
  const equal = (a,b) => JSON.stringify([...a].sort())===JSON.stringify([...b].sort());
  const existing = page.items.find(row=>row.scope_type==='organization' && row.enabled && row.target_dimension===target_dimension && equal(row.actions,actions) && equal(row.scope_targets,targets));
  if (existing) return existing;

  return call('POST','/admin/data-scopes',{token:tokens.admin,body:{schema_version:2,subject_type,subject_id,resource,actions,target_dimension,scope_type:'organization',scope_targets:targets,target_mode:'explicit',include_descendants:target_dimension==='internal_org'?false:null,enabled:true}});
}
async function deny(method,path,token,body) {
  try { await call(method,path,{token,body}); }
  catch(error) { assert.ok([403,404,409,422].includes(error.status),`${path}: ${error.message}`); return; }
  assert.fail(`${path} 必须拒绝`);
}
const financeOrg = await department(`S2财务验收-${run}`);
const otherOrg = await department(`S2业务验收-${run}`);
await change({operation:'transfer_member',user_id:person('finance'),org_unit_id:financeOrg});
await change({operation:'transfer_member',user_id:person('warehouse'),org_unit_id:otherOrg});
await change({operation:'transfer_member',user_id:person('sales'),org_unit_id:otherOrg});
await scope('user',person('management'),'work_item',['manage'],'internal_org',[financeOrg]);
const balancePage = await call('GET','/admin/stock-balances?page=1&page_size=100',{token:tokens.admin});
const balance = balancePage.items.find(row=>row.id===process.env.S2_BALANCE_ID);
assert.ok(balance,'指定库存余额必须可见');
for (const [resource,actions] of [['stock_balance',['list','detail']],['stock_movement',['list']],['stock_reservation',['list']],['stock_adjustment',['list','detail','create','update','submit']]]) {
  await scope('role',ACCOUNTS.warehouse.roleId,resource,actions,'warehouse',[balance.warehouse_id]);
}
// 库存提交同时重验 approval_instance:read，仓储角色无默认审批范围，必须显式配置仓库维度。
await scope('role',ACCOUNTS.warehouse.roleId,'approval_instance',['read'],'warehouse',[balance.warehouse_id]);
for (const role of [ACCOUNTS.finance.roleId,ACCOUNTS.management.roleId]) {
  await scope('role',role,'stock_adjustment',['list','detail'],'warehouse',[balance.warehouse_id]);
}
const stock = await call('POST','/admin/stock-adjustments',{token:tokens.warehouse,body:{balance_id:balance.id,expected_balance_version:String(balance.version),adjustment_no:`S2-${run}`,warehouse_id:balance.warehouse_id,reason_type:'STOCK_GAIN',lines:[{sku_id:balance.sku_id,quantity:'2',direction:'INCREASE'}],note:'S2范围业务验收',occurred_at:Math.floor(Date.now()/1000)}});
stock.id = stock.adjustment.id;
assert.ok(stock.id);
const detail = await call('GET',`/admin/stock-adjustments/${stock.id}`,{token:tokens.warehouse});
const submit = detail.approval.submit_command;
assert.ok(submit,'仓储账号应取得提交令牌');
await call('POST',`/admin/stock-adjustments/${stock.id}/submit`,{token:tokens.warehouse,body:{expected_version:String(submit.expected_version),expected_subject_version:String(submit.expected_subject_version),reason_type:'STOCK_GAIN',lines:detail.lines.map(line=>({line_id:line.id,quantity:'2',direction:'INCREASE'})),balances:[{balance_id:balance.id,expected_version:String(balance.version)}],note:'S2范围业务验收',occurred_at:Math.floor(Date.now()/1000),idempotency_key:randomUUID()}});
const queue = (key,view) => call('GET',`/admin/work-items?scope=${view}&page=1&page_size=100`,{token:tokens[key]});
const own = await queue('finance','mine');
const task = own.items.find(row=>row.business_object_id===stock.id);
assert.ok(task,'审批必须指定给种子财务账号');
assert.equal(task.owner_user_id,person('finance'));
assert.equal(task.owner_organization_id,balance.warehouse_id,'责任组织必须保持仓库');
let managed = await queue('management','managed');
assert.ok(managed.items.some(row=>row.id===task.id),'经理按当前人员部门可监督');
await deny('POST','/admin/approval-decisions',tokens.management,{work_item_id:task.id,decision:'APPROVE',expected_task_version:task.task_version,idempotency_key:randomUUID()});
await deny('POST',`/admin/work-items/${task.id}/reassign`,tokens.admin,{expected_task_version:task.task_version,target_user_id:person('sales'),reason:'审批禁止转交',idempotency_key:randomUUID()});
console.log('PASS seeded_accounts_stock_binding_submit_manager_no_proxy_and_no_approval_transfer');
await change({operation:'transfer_member',user_id:person('finance'),org_unit_id:otherOrg});
try {
  managed = await queue('management','managed');
  assert.ok(!managed.items.some(row=>row.id===task.id),'人员调岗后经理不能按冻结仓库越界监督');
} catch (error) {
  // 空管理集合按合同保持拒绝，403 同样证明不能越界监督。
  assert.equal(error.status,403,'调岗后管理视图应为空或拒绝');
}
assert.equal((await queue('finance','mine')).items.find(row=>row.id===task.id).owner_user_id,person('finance'));
await change({operation:'transfer_member',user_id:person('finance'),org_unit_id:financeOrg});
console.log('PASS seeded_accounts_current_member_scope_no_automatic_reassignment');
// 内部部门的同名 ID 不得充当仓库；配置合法内部部门上限后该库存审批必须失效。
const cap = await scope('user',person('finance'),'approval_instance',['read','decide'],'internal_org',[financeOrg]);
managed = await queue('management','managed');
const blocked = managed.items.find(row=>row.id===task.id);
assert.equal(blocked.processing_state,'EXECUTION_BLOCKED');
assert.equal(blocked.owner_user_id,person('finance'));
assert.ok(!blocked.allowed_actions.includes('APPROVE'));
await deny('POST','/admin/approval-decisions',tokens.finance,{work_item_id:task.id,decision:'APPROVE',expected_task_version:task.task_version,idempotency_key:randomUUID()});
await call('DELETE',`/admin/data-scopes/${cap.id}`,{token:tokens.admin});
// 个人上限删除后，已受阻实例需显式恢复当前审批人；恢复后任务应重新可见。
// 管理读取要求管理侧 read 覆盖对象，先用管理员执行恢复，保证被审人资格恢复可验证。
const instanceId = blocked.approval_context?.instance_id;
assert.ok(instanceId,'受阻任务应携带审批实例');
const resume = await call('GET',`/admin/approval-instances/${instanceId}/recovery-options`,{token:tokens.admin});
assert.ok((resume.actions||[]).includes('RESUME_CURRENT_APPROVER'),'撤销上限后应允许恢复当前审批人');
// 受阻恢复需要实例/执行/绑定三元组版本，直接取自恢复选项的版本提示；恢复响应返回下一开放任务，用该任务版本继续完成决定。
assert.ok(resume.expected_instance_version,'恢复选项应返回期望实例版本');
assert.ok(resume.expected_execution_version,'恢复选项应返回期望执行版本');
assert.ok(resume.expected_assignment_version,'恢复选项应返回期望绑定版本');
const resumed = await call('POST',`/admin/approval-instances/${instanceId}/resume-current-approver`,{token:tokens.admin,body:{expected_instance_version:resume.expected_instance_version,expected_execution_version:resume.expected_execution_version,expected_assignment_version:resume.expected_assignment_version,expected_closed_task_version:resume.expected_closed_task_version ?? undefined,idempotency_key:randomUUID()}});
assert.ok(resumed.next_open_task?.work_item_id,'恢复应返回下一开放任务');
// 原关闭任务保持不可变，恢复必须创建新执行与新任务，不得复用旧任务 ID。
assert.notEqual(resumed.next_open_task.work_item_id, task.id);
const restored = { work_item_id: resumed.next_open_task.work_item_id, owner_user_id: resumed.next_open_task.owner_user_id, task_version: resumed.next_open_task.task_version };
assert.equal(restored.owner_user_id,person('finance'));
const refreshed = (await queue('finance','mine')).items.find(row=>row.id===restored.work_item_id);
assert.ok(refreshed,'恢复后的新任务应对财务可见');
assert.equal(refreshed.business_object_id,stock.id);
await call('POST','/admin/approval-decisions',{token:tokens.finance,body:{work_item_id:restored.work_item_id,decision:'APPROVE',expected_task_version:restored.task_version,idempotency_key:randomUUID()}});
const result = await call('GET',`/admin/stock-adjustments/${stock.id}`,{token:tokens.warehouse});
assert.equal(result.adjustment.status,'POSTED');
console.log('PASS seeded_accounts_dimension_isolation_revoked_approval_blocked_and_restored');
console.log(JSON.stringify({stock_id:stock.id,work_item_id:restored.work_item_id,result:result.adjustment.status,accounts:['cangchu','caiwu','guanli','xiaoshou']}));
