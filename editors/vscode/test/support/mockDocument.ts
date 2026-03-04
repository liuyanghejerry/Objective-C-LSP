/**
 * Minimal mock for vscode.TextDocument.
 * Allows tests to run both inside EDH (where vscode is available) and
 * via plain `tsc + node` for quick iteration.
 */
export interface MockTextDocument {
  getText(): string;
  lineCount: number;
  lineAt(line: number): { text: string; range: MockRange };
  positionAt(offset: number): MockPosition;
  uri: { toString(): string };
  languageId: string;
}

export interface MockPosition {
  line: number;
  character: number;
}

export interface MockRange {
  start: MockPosition;
  end: MockPosition;
}

/**
 * Build a minimal mock TextDocument from a plain string.
 * Covers: getText(), lineCount, lineAt(i), positionAt(offset).
 */
export function mockDocument(
  content: string,
  languageId = "objective-c"
): MockTextDocument {
  const lines = content.split("\n");

  const positionAt = (offset: number): MockPosition => {
    let remaining = offset;
    for (let i = 0; i < lines.length; i++) {
      // +1 for the newline character
      const lineLen = lines[i].length + 1;
      if (remaining < lineLen) {
        return { line: i, character: remaining };
      }
      remaining -= lineLen;
    }
    // Clamp to end of document
    const lastLine = lines.length - 1;
    return { line: lastLine, character: lines[lastLine].length };
  };

  return {
    getText: () => content,
    lineCount: lines.length,
    lineAt: (line: number) => {
      const text = lines[line] ?? "";
      return {
        text,
        range: {
          start: { line, character: 0 },
          end: { line, character: text.length },
        },
      };
    },
    positionAt,
    uri: { toString: () => "file:///mock/test.m" },
    languageId,
  };
}
