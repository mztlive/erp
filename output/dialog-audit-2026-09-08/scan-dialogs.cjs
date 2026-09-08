const fs=require('fs'),path=require('path'),cp=require('child_process');
const root='/Users/huangjiajiang/Development/erp/erp-client';const ts=require(root+'/node_modules/typescript');
const files=cp.execFileSync('rg',['--files','-g','*.ts','-g','*.tsx','-g','!*.test.*','-g','!*.spec.*','-g','!tests/**','-g','!e2e/**'],{cwd:root,encoding:'utf8'}).trim().split('\n');
const options={baseUrl:root,paths:{'@/*':['./*']},moduleResolution:ts.ModuleResolutionKind.Bundler};
const data=files.map(file=>{let text=fs.readFileSync(path.join(root,file),'utf8'),sf=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);let tags=[],native=[],imports=[],copy=[];
function walk(n){const line=sf.getLineAndCharacterOfPosition(n.getStart()).line+1;
if(ts.isImportDeclaration(n)||ts.isExportDeclaration(n)){if(n.moduleSpecifier){let spec=n.moduleSpecifier.text,res=ts.resolveModuleName(spec,path.join(root,file),options,ts.sys).resolvedModule;imports.push({spec,file:res?path.relative(root,res.resolvedFileName):null,line});}}
if(ts.isJsxOpeningElement(n)||ts.isJsxSelfClosingElement(n)){let tag=n.tagName.getText(sf);if(/Dialog$|Modal$/.test(tag)||['Dialog','AlertDialog'].includes(tag))tags.push({tag,line});}
if(ts.isCallExpression(n)&& /^(window\.)?(confirm|alert|prompt)$/.test(n.expression.getText(sf)))native.push({line,text:n.getText(sf)});
if(ts.isJsxText(n)||ts.isStringLiteral(n)||ts.isNoSubstitutionTemplateLiteral(n)){if(/[\u4e00-\u9fff]/.test(n.text))copy.push({line,text:n.text.trim()});}
ts.forEachChild(n,walk);}walk(sf);return {file,tags,native,imports,copy};});
const candidates=data.filter(d=>d.tags.length||d.native.length||d.imports.some(i=>['@/components/ui/dialog','@/components/ui/alert-dialog'].includes(i.spec)));
for(const d of candidates)d.callers=data.flatMap(x=>x.imports.filter(i=>i.file===d.file).map(i=>({file:x.file,line:i.line})));
fs.writeFileSync('/tmp/erp-dialog-ast.json',JSON.stringify(candidates,null,2));
console.log(JSON.stringify({productionFiles:files.length,files:candidates.length,primitiveHosts:candidates.filter(d=>d.tags.some(t=>['Dialog','AlertDialog'].includes(t.tag))).length,primitiveRoots:candidates.flatMap(d=>d.tags).filter(t=>['Dialog','AlertDialog'].includes(t.tag)).length,formalSites:candidates.flatMap(d=>d.tags).filter(t=>t.tag==='FormalActionConfirmDialog').length,nativeSites:candidates.flatMap(d=>d.native).length}));
console.log(candidates.map(d=>`${d.file}: ${d.tags.map(t=>t.tag+'@'+t.line).join(', ')}${d.native.length?' NATIVE':''}`).join('\n'));
