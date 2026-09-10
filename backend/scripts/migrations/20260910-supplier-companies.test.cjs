// 只使用内存替身验证迁移前置条件，不连接数据库。
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const source=fs.readFileSync(__dirname+'/20260910-supplier-companies.mongosh.js','utf8');
function check({mappings=[],parties=[],revisions=[],profiles=[],suppliers=[],mode='check',unknownContract=false,unknownContractRevision=false}={}) {
  const output=[];
  const context={process:{env:{ERP_COMPANY_MAPPING_FILE:'mapping.json',ERP_COMPANY_MIGRATION_MODE:mode,ERP_MIGRATION_ACTOR:'test-only'}}, require:()=>({readFileSync:()=>JSON.stringify(mappings)}),
    EJSON:{stringify:JSON.stringify}, print:line=>output.push(JSON.parse(line)), db:{
      parties:{find:()=>parties.filter(p=>p.company_profile), findOne:q=>parties.find(p=>p.id===q.id&&!p.deleted_at)},
      party_revisions:{findOne:q=>revisions.find(r=>r.id===q.id&&r.party_id===q.party_id&&!r.deleted_at)},
      supplier_accounts:{find:()=>suppliers},supplier_commercial_profile_revisions:{findOne:q=>profiles.find(p=>p.id===q.id)},
      supplier_qualifications:{findOne:()=>unknownContract ? {valid_from:null} : null},
      supplier_qualification_revisions:{findOne:()=>unknownContractRevision ? {valid_from:null} : null},
    }};
  vm.runInNewContext(source,context);return output[0];
}
const party={id:'p1',party_kind:'enterprise',current_revision_id:'r1',deleted_at:0};
const revision={id:'r1',party_id:'p1',legal_name:'公司（甲）',short_name:null,deleted_at:0};
test('空库检查不触发任何写入',()=>assert.equal(check().companyRegistrations,0));
test('明确映射保留既有主体身份并满足历史引用',()=>{
  const result=check({mappings:[{party_id:'p1',aliases:[' 公司甲 ']}],parties:[party],revisions:[revision],suppliers:[{current_commercial_profile_revision_id:'s1'}],profiles:[{id:'s1',signing_entity_party_id:'p1',payment_entity_party_id:'p1'}]});
  assert.equal(result.companyRegistrations,1);assert.deepEqual(result.unmappedCompanyPartyIds,[]);
});
test('未映射的历史引用必须阻止上线',()=>assert.throws(()=>check({suppliers:[{current_commercial_profile_revision_id:'s1'}],profiles:[{id:'s1',signing_entity_party_id:'missing'}]}),/尚未完成/));
test('归一化别名不能占用其他公司的名称',()=>assert.throws(()=>check({mappings:[{party_id:'p1',aliases:[]}],parties:[party,{id:'p2',company_profile:{names:['公司(甲)']}}],revisions:[revision]}),/冲突/));
test('不得用不存在的主体或缺少修订的主体登记公司',()=>{
  assert.throws(()=>check({mappings:[{party_id:'p1',aliases:[]}]}),/不存在/);
  assert.throws(()=>check({mappings:[{party_id:'p1',aliases:[]}],parties:[party]}),/当前法定名称/);
});
test('当前合同或历史修订存在未知起始日时，回滚在任何写入前中止',()=>{
  assert.throws(()=>check({mode:'rollback',unknownContract:true}),/旧程序不能读取/);
  assert.throws(()=>check({mode:'rollback',unknownContractRevision:true}),/旧程序不能读取/);
});
