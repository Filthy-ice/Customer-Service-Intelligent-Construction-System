// M0 契约自检：node validate.js（需 ajv@8 + ajv-formats，NODE_PATH 指向安装目录即可）
const AjvClass = require('ajv/dist/2020');
const addFormats = require('ajv-formats');
const fs = require('fs');
const path = require('path');

const SchemasDir = __dirname;
const pairs = [
  ['rules.schema.json', 'examples/rules.sample.json'],
  ['flows.schema.json', 'examples/flows.sample.json'],
  ['data-dictionary.schema.json', 'examples/dictionary.sample.json'],
  ['api-contract.schema.json', 'examples/apis.sample.json'],
  ['skills.schema.json', 'examples/skills.sample.json'],
  ['eval-cases.schema.json', 'examples/eval.sample.json'],
  ['pack.manifest.schema.json', 'examples/pack.sample.json'],
  ['pipeline-state.schema.json', 'examples/pipeline-state.sample.json'],
];

const ajv = new AjvClass({ strict: 'log', allErrors: true });
addFormats(ajv);
for (const [s] of pairs) {
  ajv.addSchema(JSON.parse(fs.readFileSync(path.join(SchemasDir, s), 'utf8')));
}

let failed = 0;
for (const [s, d] of pairs) {
  const schema = ajv.getSchema(`icewright://${s}`);
  const data = JSON.parse(fs.readFileSync(path.join(SchemasDir, d), 'utf8'));
  if (schema(data)) {
    console.log(`PASS  ${d}  <-  ${s}`);
  } else {
    failed++;
    console.log(`FAIL  ${d}  <-  ${s}`);
    for (const e of schema.errors) {
      console.log(`   ${e.instancePath || '(root)'} ${e.message} ${e.params ? JSON.stringify(e.params) : ''}`);
    }
  }
}
process.exit(failed ? 1 : 0);
