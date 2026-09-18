// Real MCP client check using the official TypeScript SDK over Streamable HTTP.
// Usage: node scripts/mcp-sdk-check.mjs [MCP_URL] [TOKEN]
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import assert from 'node:assert/strict';

const url = process.argv[2] || 'http://127.0.0.1:8795/mcp';
const token = process.argv[3] || '';
const headers = token ? { Authorization: `Bearer ${token}` } : undefined;

const transport = new StreamableHTTPClientTransport(new URL(url), {
  requestInit: { headers },
});
const client = new Client({ name: 'scholargateway-sdk-check', version: '1.0.0' });

await client.connect(transport);

const { tools } = await client.listTools();
assert.equal(tools.some(tool => tool.name === 'list_workspaces'), false);
assert.ok(tools.find(tool => tool.name === 'list_interested_papers')?.inputSchema.properties.fields);
console.log(`tools (${tools.length}): ${tools.map((tool) => tool.name).join(', ')}`);

const catalog = await client.callTool({ name: 'get_search_catalog', arguments: {} });
const catalogText = catalog.content?.[0]?.text ?? '';
const catalogData = JSON.parse(catalogText);
console.log(`catalog: ${catalogData.sources?.length ?? 0} sources, ${catalogData.presets?.length ?? 0} presets`);

const interested = await client.callTool({ name: 'list_interested_papers', arguments: { limit: 2 } });
const libraryData = JSON.parse(interested.content[0].text);
assert.equal(interested.isError, false);
assert.ok(libraryData.papers.length <= 2);
assert.equal(libraryData.limit, 2);
for (const item of libraryData.papers) {
  assert.equal(Object.hasOwn(item.paper, 'abstract'), false);
  assert.ok(Object.hasOwn(item.paper, 'source_url'));
}
console.log(`interest library: ${libraryData.total} papers`);

await client.close();
console.log('MCP SDK check OK');
