<script lang="ts">
  import { onMount } from 'svelte'
  import { resolve } from '$app/paths'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxAi, phxAssistant, type PhxAiConfigView, type PhxAiOp, type PhxOpResult } from '$lib/ipc'

  /**
   * AI 助手（稿图 29，0.2.0 第 4 段）。跨域指挥台：一句话 → 模型经 apply_ops 工具
   * 产出白名单操作计划 → 操作预览卡逐项确认 → assistant_execute 执行（各域 save 链自带备份）。
   * 隐私：请求只带最小目录上下文（assistant_context），不带完整存档。
   */

  let ai = $state<PhxAiConfigView | null>(null)
  let ctxReady = $state(false)

  interface Msg {
    role: 'user' | 'ai'
    text: string
  }
  let msgs = $state<Msg[]>([])
  let input = $state('')
  let thinking = $state(false)

  // —— 操作预览卡状态 ——
  let plan = $state<{ ops: { op: PhxAiOp; on: boolean }[]; note: string } | null>(null)
  let executing = $state(false)
  let receipt = $state<{ ok: number; fail: string[] } | null>(null)

  const chips = $derived([
    t('assistant.chip1'),
    t('assistant.chip2'),
    t('assistant.chip3'),
    t('assistant.chip4')
  ])

  // —— 工具 schema：apply_ops（六类白名单；宽松 object，模型兼容性优先）——
  function toolsSchema(): unknown[] {
    return [
      {
        type: 'function',
        function: {
          name: 'apply_ops',
          description:
            '提出一组存档编辑操作计划（不会立即执行，用户确认后才会执行）。一次可以包含多条。',
          parameters: {
            type: 'object',
            properties: {
              ops: {
                type: 'array',
                description: '操作列表',
                items: {
                  type: 'object',
                  properties: {
                    kind: {
                      type: 'string',
                      enum: [
                        'skillsAll',
                        'tradersMax',
                        'questsComplete',
                        'moneySet',
                        'clothingUnlockAll',
                        'setLook',
                        'arsenalApply'
                      ]
                    },
                    level: { type: 'number', description: 'skillsAll：技能等级 1-51' },
                    qids: { type: 'array', items: { type: 'string' }, description: 'questsComplete：任务 qid 列表（从任务目录查）' },
                    rub: { type: 'number', description: 'moneySet：卢布目标值' },
                    eur: { type: 'number', description: 'moneySet：欧元目标值' },
                    usd: { type: 'number', description: 'moneySet：美元目标值' },
                    head: { type: 'string', description: '头型名（从头型目录查）' },
                    voice: { type: 'string', description: '语音名（从语音目录查）' },
                    setId: { type: 'string', description: 'arsenalApply：军械库套餐 id' },
                    weaponOnly: { type: 'boolean', description: 'arsenalApply：是否仅武器' }
                  },
                  required: ['kind']
                }
              }
            },
            required: ['ops']
          }
        }
      }
    ]
  }

  function systemPrompt(ctx: Awaited<ReturnType<typeof phxAssistant.context>>): string {
    const quests = ctx.quests.slice(0, 180).map(([q, n]) => `${q}=${n}`).join('；')
    return [
      `你是 Phoenix-DRX（SPT 存档编辑器）的 AI 助手，版本 ${ctx.version}。用户会用中文提出存档修改需求，你把它转换成 apply_ops 工具调用（可多条操作）。`,
      '可用操作（kind）：skillsAll 全部技能到 N 级；tradersMax 全商人忠诚4+好感1；questsComplete 完成 qid 对应任务；moneySet 货币调到目标值；clothingUnlockAll 解锁全部服装（可带 head/voice）；setLook 只换头型/语音；arsenalApply 应用军械库套餐（setId 见目录）。',
      '规则：只通过 apply_ops 表达改动；不要编造 qid/setId/头型语音名——从下方目录里查；用户要求含糊时（比如"给点钱"）给合理默认并在文字里说明；执行前用户会逐项确认，你只负责提计划。',
      `任务目录（qid=名，最多180条）：${quests}`,
      `军械库套餐（id=名）：${ctx.arsenal.map(([i, n]) => `${i}=${n}`).join('；')}`,
      `头型目录：${ctx.heads.join('、')}｜语音目录：${ctx.voices.join('、')}`,
      '技能与商人支持"全部"类操作，无需点名。回复一律简体中文，简短说明计划要点。'
    ].join('\n')
  }

  let history: { role: 'system' | 'user' | 'assistant'; content: string }[] = []

  async function send(text?: string): Promise<void> {
    const q = (text ?? input).trim()
    if (!q || thinking) return
    input = ''
    msgs = [...msgs, { role: 'user', text: q }]
    history.push({ role: 'user', content: q })
    thinking = true
    plan = null
    receipt = null
    try {
      const msg = await phxAi.chat({
        messages: history,
        tools: toolsSchema()
      })
      const content = typeof msg.content === 'string' ? msg.content : ''
      const calls = (msg.tool_calls ?? []) as { function?: { arguments?: string } }[]
      if (calls.length > 0) {
        let ops: PhxAiOp[] = []
        for (const c of calls) {
          try {
            const args = JSON.parse(c.function?.arguments ?? '{}') as { ops?: PhxAiOp[] }
            if (Array.isArray(args.ops)) ops = ops.concat(args.ops)
          } catch { /* 单条解析失败跳过 */ }
        }
        const valid = ops.filter(isValidOp)
        plan = { ops: valid.map((op) => ({ op, on: true })), note: content }
        msgs = [...msgs, { role: 'ai', text: content || t('assistant.planReady') }]
        history.push({ role: 'assistant', content: content || t('assistant.planReady') })
      } else {
        msgs = [...msgs, { role: 'ai', text: content || t('assistant.empty') }]
        history.push({ role: 'assistant', content })
      }
    } catch (e) {
      msgs = [...msgs, { role: 'ai', text: t('assistant.error', { msg: String(e) }) }]
    } finally {
      thinking = false
    }
  }

  function isValidOp(op: unknown): op is PhxAiOp {
    const o = op as { kind?: string }
    return !!o && typeof o.kind === 'string' && [
      'skillsAll', 'tradersMax', 'questsComplete', 'moneySet', 'clothingUnlockAll', 'setLook', 'arsenalApply'
    ].includes(o.kind)
  }

  function opSummary(op: PhxAiOp): string {
    switch (op.kind) {
      case 'skillsAll': return t('assistant.op.skillsAll', { level: op.level })
      case 'tradersMax': return t('assistant.op.tradersMax')
      case 'questsComplete': return t('assistant.op.quests', { n: op.qids.length })
      case 'moneySet': {
        const parts: string[] = []
        if (op.rub != null) parts.push(t('assistant.op.rub', { n: op.rub }))
        if (op.eur != null) parts.push(t('assistant.op.eur', { n: op.eur }))
        if (op.usd != null) parts.push(t('assistant.op.usd', { n: op.usd }))
        return parts.join(' · ')
      }
      case 'clothingUnlockAll': return t('assistant.op.clothing')
      case 'setLook': return t('assistant.op.look', {
        head: op.head ?? '—',
        voice: op.voice ?? '—'
      })
      case 'arsenalApply': return t('assistant.op.arsenal', { id: op.setId })
    }
  }

  async function executePlan(): Promise<void> {
    if (!plan || executing) return
    const chosen = plan.ops.filter((o) => o.on).map((o) => o.op)
    if (chosen.length === 0) return
    executing = true
    try {
      const rs: PhxOpResult[] = await phxAssistant.execute(chosen)
      const fail = rs.filter((r) => !r.ok && r.error).map((r) => r.error as string)
      receipt = { ok: rs.filter((r) => r.ok).length, fail }
      if (fail.length === 0) toast.success(t('assistant.execDone'))
      else toast.error(t('assistant.execPartial', { n: fail.length }))
      plan = null
      history.push({ role: 'assistant', content: t('assistant.execDone') })
    } catch (e) {
      toast.error(String(e))
    } finally {
      executing = false
    }
  }

  onMount(async () => {
    try {
      ai = await phxAi.getConfig()
      if (ai.enabled) {
        const ctx = await phxAssistant.context()
        history = [{ role: 'system', content: systemPrompt(ctx) }]
        ctxReady = true
        msgs = [{ role: 'ai', text: t('assistant.hello') }]
      }
    } catch {
      ai = null
    }
  })
</script>

<div class="page">
  <div class="main-head">
    <div>
      <div class="large-title">{t('nav.assistant')}</div>
      <div class="head-meta">{t('assistant.subtitle')}</div>
    </div>
    <div class="head-meta">{t('assistant.safe')}</div>
  </div>

  {#if !ai?.enabled}
    <!-- 未启用/未配置：引导态（诚实原则——没有假对话） -->
    <div class="guide">
      <div class="g-title">{t('assistant.guide.title')}</div>
      <div class="g-desc">{t('assistant.guide.desc')}</div>
      <a class="g-btn" href={resolve('/settings')}>{t('assistant.guide.go')}</a>
    </div>
  {:else if !ctxReady}
    <div class="guide"><div class="g-desc">{t('assistant.guide.loading')}</div></div>
  {:else}
    <div class="chat">
      {#each msgs as m, i (i)}
        <div class="msg" class:user={m.role === 'user'}>
          {#if m.role === 'ai'}<div class="avatar">P</div>{/if}
          <div class="bubble">{m.text}</div>
        </div>
      {/each}
      {#if thinking}
        <div class="msg"><div class="avatar">P</div><div class="bubble dim">{t('assistant.thinking')}</div></div>
      {/if}

      {#if plan}
        <div class="plan">
          <div class="plan-h">
            <div class="plan-t">{t('assistant.plan.title', { n: plan.ops.length })}</div>
            <div class="plan-sub">{t('assistant.plan.sub')}</div>
          </div>
          {#each plan.ops as row, i (i)}
            <button class="op" type="button" class:off={!row.on} onclick={() => (row.on = !row.on)}>
              <span class="cb" class:on={row.on}></span>
              <span class="op-t">{opSummary(row.op)}</span>
            </button>
          {/each}
          <div class="plan-f">
            <span class="plan-note">{t('assistant.plan.backup')}</span>
            <button class="btn ghost" type="button" onclick={() => (plan = null)}>{t('assistant.plan.drop')}</button>
            <button
              class="btn primary"
              type="button"
              disabled={executing || plan.ops.filter((o) => o.on).length === 0}
              onclick={() => void executePlan()}
            >
              {executing ? t('assistant.executing') : t('assistant.plan.run', { n: plan.ops.filter((o) => o.on).length })}
            </button>
          </div>
        </div>
      {/if}

      {#if receipt}
        <div class="receipt" class:bad={receipt.fail.length > 0}>
          {#if receipt.fail.length === 0}
            {t('assistant.receipt.ok', { n: receipt.ok })}
          {:else}
            {t('assistant.receipt.partial', { ok: receipt.ok, fail: receipt.fail.length })}
            <div class="r-err">{receipt.fail.join('；')}</div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="chips">
      {#each chips as c (c)}<button class="chip-s" type="button" onclick={() => void send(c)}>{c}</button>{/each}
    </div>
    <div class="inputbar">
      <input
        class="tin"
        placeholder={t('assistant.placeholder')}
        bind:value={input}
        onkeydown={(e) => { if (e.key === 'Enter') void send() }}
      />
      <button class="send" type="button" aria-label={t('assistant.send')} disabled={thinking} onclick={() => void send()}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>
      </button>
    </div>
    <div class="foot-hint">{t('assistant.footHint')}</div>
  {/if}
</div>

<style>
  .page { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 40px 0 92px; }
  .main-head { padding: 18px 24px 0; flex: none; display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; }
  .large-title { font-size: 34px; font-weight: 700; letter-spacing: -0.4px; }
  .head-meta { font-size: 13px; color: var(--phx-text-2); margin-top: 6px; }
  .guide { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; }
  .g-title { font-size: 18px; font-weight: 700; }
  .g-desc { font-size: 13px; color: var(--phx-text-2); max-width: 420px; text-align: center; line-height: 1.7; }
  .g-btn {
    margin-top: 8px; height: 34px; padding: 0 18px; border-radius: var(--phx-r-btn);
    background: var(--phx-accent); color: #fff; font-size: 13px; font-weight: 600;
    display: inline-flex; align-items: center; text-decoration: none;
  }
  .chat { flex: 1; min-height: 0; overflow-y: auto; margin-top: 12px; padding: 8px 24px 12px; scrollbar-width: none; }
  .chat::-webkit-scrollbar { display: none; }
  .msg { display: flex; margin-top: 18px; gap: 12px; }
  .msg.user { justify-content: flex-end; }
  .bubble { max-width: 560px; border-radius: 14px; padding: 12px 16px; font-size: 13.5px; line-height: 1.7; white-space: pre-wrap; }
  .msg.user .bubble { background: var(--phx-accent); color: #fff; border-bottom-right-radius: 4px; }
  .msg .bubble { background: var(--phx-surface); border-bottom-left-radius: 4px; }
  .bubble.dim { color: var(--phx-text-2); }
  .avatar {
    width: 34px; height: 34px; border-radius: 10px; flex: none; margin-top: 2px;
    display: flex; align-items: center; justify-content: center;
    font-family: fantasy; font-style: italic; font-weight: 700; font-size: 17px;
    background: var(--phx-accent-dim); color: var(--phx-accent);
  }
  .plan { max-width: 640px; background: var(--phx-surface-2); border-radius: var(--phx-r-card); overflow: hidden; margin: 12px 0 0 46px; }
  .plan-h { padding: 12px 16px 8px; }
  .plan-t { font-size: 14px; font-weight: 700; }
  .plan-sub { font-size: 11.5px; color: var(--phx-text-3); margin-top: 2px; }
  .op {
    display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 16px;
    border: none; border-top: 0.5px solid var(--phx-sep); background: transparent;
    cursor: pointer; text-align: left; font-family: inherit; color: var(--phx-text);
  }
  .op.off .op-t { color: var(--phx-text-3); text-decoration: line-through; }
  .cb { width: 16px; height: 16px; border-radius: 5px; flex: none; background: var(--phx-fill-quaternary); position: relative; }
  .cb.on { background: var(--phx-accent); }
  .cb.on::after {
    content: ''; position: absolute; left: 5px; top: 2px; width: 4px; height: 8px;
    border: solid #fff; border-width: 0 1.6px 1.6px 0; transform: rotate(45deg);
  }
  .op-t { font-size: 13px; font-weight: 600; }
  .plan-f { padding: 12px 16px; border-top: 0.5px solid var(--phx-sep); display: flex; align-items: center; gap: 10px; }
  .plan-note { font-size: 11px; color: var(--phx-text-3); flex: 1; }
  .btn { height: 32px; padding: 0 14px; border: none; border-radius: var(--phx-r-btn); font-size: 12.5px; font-weight: 600; font-family: inherit; cursor: pointer; display: inline-flex; align-items: center; }
  .btn.primary { background: var(--phx-accent); color: #fff; }
  .btn.ghost { background: var(--phx-fill-quaternary); color: var(--phx-text-2); }
  .receipt { margin: 10px 0 0 46px; font-size: 12.5px; color: var(--phx-green); }
  .receipt.bad { color: var(--phx-warn); }
  .r-err { margin-top: 4px; color: var(--phx-text-3); font-size: 11.5px; }
  .chips { display: flex; gap: 8px; padding: 4px 24px 8px; flex: none; flex-wrap: wrap; }
  .chip-s { font-size: 12px; color: var(--phx-text-2); background: var(--phx-surface); border: none; border-radius: 9px; padding: 6px 12px; cursor: pointer; font-family: inherit; }
  .chip-s:hover { color: var(--phx-text); background: var(--phx-surface-2); }
  .inputbar { flex: none; margin: 0 24px 8px; display: flex; align-items: center; gap: 10px; background: var(--phx-surface); border-radius: 14px; padding: 8px 8px 8px 16px; box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong); }
  .tin { flex: 1; border: none; background: transparent; color: var(--phx-text); font-size: 13.5px; font-family: inherit; outline: none; height: 36px; }
  .send { width: 38px; height: 38px; border-radius: 11px; background: var(--phx-accent); border: none; display: flex; align-items: center; justify-content: center; flex: none; cursor: pointer; }
  .send svg { width: 17px; height: 17px; color: #fff; }
  .send:disabled { opacity: 0.5; }
  .foot-hint { flex: none; text-align: center; font-size: 11px; color: var(--phx-text-3); padding-bottom: 6px; }
</style>
