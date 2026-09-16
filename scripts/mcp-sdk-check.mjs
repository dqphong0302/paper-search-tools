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
assert.ok(tools.find(tool => tool.name === 'search_academic_papers')?.inputSchema.properties.workspace_id);
assert.ok(tools.find(tool => tool.name === 'get_workspace')?.inputSchema.properties.fields);
console.log(`tools (${tools.length}): ${tools.map((tool) => tool.name).join(', ')}`);

const catalog = await client.callTool({ name: 'get_search_catalog', arguments: {} });
const catalogText = catalog.content?.[0]?.text ?? '';
const catalogData = JSON.parse(catalogText);
console.log(`catalog: ${catalogData.sources?.length ?? 0} sources, ${catalogData.presets?.length ?? 0} presets`);

const workspaces = await client.callTool({ name: 'list_workspaces', arguments: {} });
const wsData = JSON.parse(workspaces.content[0].text);
console.log(`workspaces: ${wsData.workspaces.length} (first: ${wsData.workspaces[0]?.name ?? '—'})`);

if (wsData.workspaces[0]) {
  const detail = await client.callTool({
    name: 'get_workspace',
    arguments: { workspace_id: wsData.workspaces[0].id, query_limit: 5, limit: 2 },
  });
  const detailData = JSON.parse(detail.content[0].text);
  assert.equal(detail.isError, false);
  assert.ok(detailData.papers.length <= 2);
  assert.equal(detailData.limit, 2);
  for (const item of detailData.papers) {
    assert.equal(Object.hasOwn(item.paper, 'abstract'), false);
    assert.ok(Object.hasOwn(item.paper, 'source_url'));
  }
  if (detailData.next_offset !== null) {
    const next = await client.callTool({ name: 'get_workspace', arguments: {
      workspace_id: wsData.workspaces[0].id, offset: detailData.next_offset, limit: 2, fields: ['abstract'],
    } });
    assert.equal(next.isError, false);
    const nextData = JSON.parse(next.content[0].text);
    assert.equal(nextData.offset, detailData.next_offset);
    for (const item of nextData.papers) {
      assert.ok(Object.hasOwn(item.paper, 'abstract'));
      assert.ok(!detailData.papers.some(previous => previous.paper.id === item.paper.id));
    }
  }
  console.log(`workspace detail: ${detailData.papers.length} papers, ${detailData.recent_queries.length} queries`);
}

await client.close();
console.log('MCP SDK check OK');
