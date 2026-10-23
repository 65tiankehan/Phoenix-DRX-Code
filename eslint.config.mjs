import js from '@eslint/js'
import svelte from 'eslint-plugin-svelte'
import tseslint from 'typescript-eslint'
import globals from 'globals'

export default tseslint.config(
  {
    ignores: [
      '.svelte-kit/**',
      'node_modules/**',
      'src-tauri/target/**',
      'installer/src-tauri/target/**',
      '.legacy/**',
      'docs/**',
      'out/**',
      'dist/**',
      'build/**',
      'resources/**'
    ]
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    // svelte-eslint-parser 对 script 内容委托子解析器；lang="ts" / .svelte.ts 必须接 TS parser，
    // 否则 espree 解析 TS 语法直接报 Parsing error
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser
      }
    }
  },
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node }
    }
  },
  {
    rules: {
      // ==== 原生件卡点（AGENTS.md §三：原生 confirm/alert/prompt、new Audio、原生 title 一律禁用） ====
      // 1-4. 原生对话框与音效：全文件生效（.ts 与 .svelte 的 script 均为 estree 节点）
      'no-restricted-syntax': [
        'error',
        {
          selector:
            "CallExpression[callee.object.name='window'][callee.property.name=/^(confirm|alert|prompt)$/]",
          message: '禁用 window.confirm/alert/prompt，请改用 PhxDialog（视觉基线原生件替换表）'
        },
        {
          selector: "CallExpression[callee.name=/^(confirm|alert|prompt)$/]",
          message: '禁用裸 confirm/alert/prompt，请改用 PhxDialog（视觉基线原生件替换表）'
        },
        {
          selector: "NewExpression[callee.name='Audio']",
          message: '禁用 new Audio，音效方案见 docs/design/感官层基线.md'
        },
        {
          selector: "NewExpression[callee.object.name='window'][callee.property.name='Audio']",
          message: '禁用 new window.Audio，音效方案见 docs/design/感官层基线.md'
        },
        // 5-6. 原生 title（Svelte AST）：只命中小写原生元素，不误伤 PascalCase 组件的 title 属性
        // 实测 AST：SvelteElement.name 是对象（name.name 才是标签名）；bind:title 编码为 kind='Binding' + key.name.name='title'
        {
          selector:
            "SvelteElement[name.name=/^[a-z][a-zA-Z0-9-]*$/] > SvelteStartTag SvelteAttribute[key.name='title']",
          message: '禁用原生 title 提示，请改用 PhxTooltip（视觉基线原生件替换表）'
        },
        {
          selector:
            "SvelteElement[name.name=/^[a-z][a-zA-Z0-9-]*$/] > SvelteStartTag SvelteDirective[kind='Binding'][key.name.name='title']",
          message: '禁用原生 title 绑定，请改用 PhxTooltip（视觉基线原生件替换表）'
        }
      ]
    }
  }
)
