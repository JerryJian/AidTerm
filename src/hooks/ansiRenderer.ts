import { Renderer, type Tokens } from 'marked'

export class AnsiRenderer extends Renderer {
  code(token: Tokens.Code): string {
    const header = token.lang ? `\x1b[90m\u2500\u2500 ${token.lang} \u2500\u2500\x1b[0m\n` : ''
    const body = token.text
      .split('\n')
      .map(l => `  \x1b[90m${l}\x1b[0m`)
      .join('\n')
    return `\n${header}${body}\n`
  }

  blockquote(token: Tokens.Blockquote): string {
    const content = this.parser.parse(token.tokens)
    return content
      .trimEnd()
      .split('\n')
      .map(l => `\x1b[2;3m${l}\x1b[0m`)
      .join('\n') + '\n'
  }

  heading(token: Tokens.Heading): string {
    const content = this.parser.parseInline(token.tokens)
    if (token.depth === 1) return `\x1b[1;36;4m${content}\x1b[0m\n`
    return `\x1b[1;36m${content}\x1b[0m\n`
  }

  hr(): string {
    return '\x1b[90m\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\x1b[0m\n'
  }

  list(token: Tokens.List): string {
    let result = ''
    for (let i = 0; i < token.items.length; i++) {
      const item = token.items[i]
      const content = this.parser.parse(item.tokens)
      const lines = content.trimEnd().split('\n')
      if (token.ordered) {
        const num = Number(token.start) + i
        result += `${num}. ${lines[0]}\n`
        for (let j = 1; j < lines.length; j++) {
          result += `   ${lines[j]}\n`
        }
      } else {
        result += `  \x1b[36m\u2022\x1b[0m ${lines[0]}\n`
        for (let j = 1; j < lines.length; j++) {
          result += `   ${lines[j]}\n`
        }
      }
    }
    return result
  }

  listitem(_token: Tokens.ListItem): string {
    return ''
  }

  checkbox(token: Tokens.Checkbox): string {
    return token.checked ? '\x1b[92m[\u2713]\x1b[0m' : '\x1b[90m[ ]\x1b[0m'
  }

  paragraph(token: Tokens.Paragraph): string {
    return this.parser.parseInline(token.tokens) + '\n'
  }

  table(token: Tokens.Table): string {
    const header = token.header.map(c => this.parser.parseInline(c.tokens))
    const rows = token.rows.map(r => r.map(c => this.parser.parseInline(c.tokens)))
    const colWidths = header.map((h, i) => {
      const dataWidths = rows.map(r => (r[i] || '').length)
      return Math.max(h.length, ...dataWidths)
    })
    const formatRow = (row: string[]) =>
      ' ' + row.map((cell, i) => cell.padEnd(colWidths[i])).join(' \x1b[90m|\x1b[0m ')

    const headerLine = formatRow(header)
    const sep = ' ' + colWidths.map(w => '\x1b[90m' + '\u2500'.repeat(w) + '\x1b[0m').join(' \x1b[90m\u2534\x1b[0m ')
    const body = rows.map(r => formatRow(r)).join('\n')
    return `${headerLine}\n${sep}\n${body}\n`
  }

  tablerow(_token: Tokens.TableRow): string {
    return ''
  }

  tablecell(_token: Tokens.TableCell): string {
    return ''
  }

  strong(token: Tokens.Strong): string {
    return '\x1b[1m' + this.parser.parseInline(token.tokens) + '\x1b[0m'
  }

  em(token: Tokens.Em): string {
    return '\x1b[3m' + this.parser.parseInline(token.tokens) + '\x1b[0m'
  }

  codespan(token: Tokens.Codespan): string {
    return '\x1b[33m' + token.text + '\x1b[0m'
  }

  br(): string {
    return '\n'
  }

  del(token: Tokens.Del): string {
    return '\x1b[9m' + this.parser.parseInline(token.tokens) + '\x1b[0m'
  }

  link(token: Tokens.Link): string {
    const content = this.parser.parseInline(token.tokens)
    return `\x1b]8;;${token.href}\x07\x1b[4;34m${content}\x1b[0m\x1b]8;;\x07`
  }

  image(token: Tokens.Image): string {
    return token.text ? `\x1b[90m[${token.text}]\x1b[0m` : ''
  }

  text(token: Tokens.Text | Tokens.Escape | Tokens.Tag): string {
    return token.text
  }
}
