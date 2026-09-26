import { describe, expect, it } from 'vitest';
import { Paper, Workspace, WorkspacePaper } from '../types';
import { agentHandoffPrompt, groupPapers, summarizeCollection, summaryMarkdown } from './collections';

const item = (id: string, extra: Partial<Paper> = {}): WorkspacePaper => ({
  paper: { id, title: `Paper ${id}`, authors: [], source: 'OpenAlex', open_access: false, ...extra },
  added_at: 0,
});

const items = [
  item('a', { quartile: 'Q2', year: 2021, biblio: { topic: 'Gene therapy' } }),
  item('b', { quartile: 'Q1', year: 2024, biblio: { topic: 'Gene therapy' }, open_access: true }),
  item('c', { year: 2023, biblio: { field: 'Medicine' } }),
];

describe('collections', () => {
  it('orders quartile groups Q1→Q4 with unranked last', () => {
    expect(groupPapers(items, 'quartile').map((g) => g.label)).toEqual(['Q1', 'Q2', 'Unranked']);
  });

  it('groups by topic, falling back to the field', () => {
    const groups = groupPapers(items, 'topic');
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([['Gene therapy', 2], ['Medicine', 1]]);
  });

  it('orders years newest first', () => {
    expect(groupPapers(items, 'year').map((g) => g.label)).toEqual(['2024', '2023', '2021']);
  });

  it('summarises coverage and writes a Markdown synthesis', () => {
    const summary = summarizeCollection(items, new Set(['a', 'b']), new Set(['a']));
    expect(summary.yearRange).toEqual([2021, 2024]);
    expect(summary.withPdf).toBe(2);
    expect(summary.withMarkdown).toBe(1);
    expect(summary.quartiles.find((q) => q.label === 'Unranked')?.count).toBe(1);
    const md = summaryMarkdown('Review', summary, groupPapers(items, 'quartile'), 'quartile');
    expect(md).toContain('- PDFs collected: 2/3');
    expect(md).toContain('### Q1 (1)');
  });

  it('tells the agent which MCP tools to call for the collection', () => {
    const collection: Workspace = { id: 'ws-1', name: 'Review', created_at: 0, updated_at: 0, paper_count: 3, query_count: 0 };
    const prompt = agentHandoffPrompt(collection, 'Compare efficacy', '/tmp/review');
    expect(prompt).toContain('collection_id: "ws-1"');
    expect(prompt).toContain('get_paper_fulltext');
    expect(prompt).toContain('/tmp/review');
    expect(prompt.endsWith('Task: Compare efficacy')).toBe(true);
  });
});
