import { describe, expect, it } from 'vitest';
import { Paper } from '../types';
import { textFromItems, tidyText, toMarkdown } from './pdfText';

const item = (str: string, y: number, hasEOL = false) => ({ str, y, hasEOL, height: 10, transform: [10, 0, 0, 10, 72, y] });

describe('PDF text to Markdown', () => {
  it('rebuilds lines and paragraphs from positioned text', () => {
    const text = textFromItems([
      item('Background ', 700), item('and aims', 700),
      item('of the study.', 688),
      item('Methods were', 650),
      item('applied.', 638),
    ]);
    expect(text).toBe('Background and aims of the study.\n\nMethods were applied.');
  });

  it('repairs words hyphenated across a line break', () => {
    expect(tidyText('gene edit-\ning works\n\nnew para')).toBe('gene editing works\n\nnew para');
    expect(tidyText('well-\nKnown')).toBe('well- Known');
  });

  it('writes YAML front matter an agent can rely on', () => {
    const paper: Paper = {
      id: 'doi:10.1/x', title: 'A "quoted" title', authors: ['Nguyễn Văn A'], year: 2024,
      venue: 'The Lancet', quartile: 'Q1', doi: '10.1/x', source: 'OpenAlex', open_access: true,
    };
    const md = toMarkdown(paper, { pages: ['First page', ''], method: 'mixed', ocrPages: 1 });
    expect(md.startsWith('---\ntitle: "A \\"quoted\\" title"\n')).toBe(true);
    expect(md).toContain('authors: ["Nguyễn Văn A"]');
    expect(md).toContain('quartile: Q1');
    expect(md).toContain('extraction: mixed');
    expect(md).toContain('## Page 1\n\nFirst page');
    expect(md).toContain('## Page 2\n\n*No text recognised on this page.*');
  });
});
