<script lang="ts">
  // 04 商人页：8 商人卡 + 跳蚤评分，按 docs/prototypes/04 稿图逐项落地。
  // 联动口径复刻参考 CharacterTraderStandingExtended.cs：
  // - 点 +/- 走等级 setter 链（起始值一次性捕获，升级补下限/降级回落，unlocked 自动放行）；
  // - 手改声望/销售只反推等级（SetLevel 直写底层，不动输入值）；无匹配段则等级不变。
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxTraders, type PhxTraderRow, type PhxTradersSaveData } from '$lib/ipc'
  import PhxSwitch from '$lib/components/PhxSwitch.svelte'

  /** 联动起始值快照（对齐参考 ??= 一次性捕获语义） */
  type StartCap = { level: number; sales: number; standing: number; unlocked: boolean }

  /** 已知商人：SPT 源码 Traders 枚举实证 ID，key 为 i18n 名，av 为稿图头像字 */
  const KNOWN: Array<{ id: string; key: MessageKey; en: string; av: string }> = [
    { id: '54cb50c76803fa8b248b4571', key: 'traders.name.prapor', en: 'Prapor', av: 'П' },
    { id: '54cb57776803fa99248b456e', key: 'traders.name.therapist', en: 'Therapist', av: 'Т' },
    { id: '58330581ace78e27b8b10cee', key: 'traders.name.skier', en: 'Skier', av: 'S' },
    { id: '5935c25fb3acc3127c3d8cd9', key: 'traders.name.peacekeeper', en: 'Peacekeeper', av: 'P' },
    { id: '5a7c2eca46aef81a7ca2145d', key: 'traders.name.mechanic', en: 'Mechanic', av: 'M' },
    { id: '5ac3b934156ae10c4430e83c', key: 'traders.name.ragman', en: 'Ragman', av: 'R' },
    { id: '5c0647fdd443bc2504c2d371', key: 'traders.name.jaeger', en: 'Jaeger', av: 'J' },
    { id: '579dc571d53a0658a154fbec', key: 'traders.name.fence', en: 'Fence', av: 'F' }
  ]

  const knownOf = (id: string) => KNOWN.find((k) => k.id === id)

  let currentId = $state('')
  let traders = $state<PhxTraderRow[]>([])
  let rating = $state(0)
  let loaded = $state(false)
  let saving = $state(false)
  /** 销售输入框显示文本（千分位） */
  let salesTexts = $state<Record<string, string>>({})
  /** 声望输入框显示文本（两位小数） */
  let standingTexts = $state<Record<string, string>>({})
  let ratingText = $state('0.00')
  /** 联动起始值（非渲染态，不进响应式） */
  let starts: Record<string, StartCap> = {}

  /** 稿图排序：已知商人按固定序，未知排后按原名 */
  const sorted = $derived.by(() => {
    const idx = new Map(KNOWN.map((k, i) => [k.id, i]))
    return [...traders].sort((a, b) => {
      const ia = idx.get(a.id) ?? KNOWN.length
      const ib = idx.get(b.id) ?? KNOWN.length
      if (ia !== ib) return ia - ib
      return a.nickname.localeCompare(b.nickname)
    })
  })

  const metaText = $derived(loaded ? t('traders.meta.loaded') : t('traders.meta.none'))

  /** 销售额千分位显示 */
  const fmtSales = (n: number): string => Math.trunc(n).toLocaleString('en-US')
  /** 声望/评分两位小数（−0 归 0.00） */
  const fmt2 = (n: number): string => (Object.is(n, -0) ? 0 : n).toFixed(2)

  function syncTexts(): void {
    const sales: Record<string, string> = {}
    const standing: Record<string, string> = {}
    for (const r of traders) {
      sales[r.id] = fmtSales(r.salesSum)
      standing[r.id] = fmt2(r.standing)
    }
    salesTexts = sales
    standingTexts = standing
    ratingText = fmt2(rating)
  }

  /** 起始值一次性捕获（参考 ??=：首次取当前值，之后沿用） */
  function captureStart(row: PhxTraderRow): StartCap {
    let s = starts[row.id]
    if (!s) {
      s = {
        level: row.loyaltyLevel,
        sales: row.salesSum,
        standing: row.standing,
        unlocked: row.unlocked
      }
      starts[row.id] = s
    }
    return s
  }

  /** 等级 setter 链（SetSalesSum/SetStanding/SetUnlocked）：升级补下限，降级回落 */
  function applyLevelLink(row: PhxTraderRow, next: number, st: StartCap): void {
    const seg = row.levels[next - 1]
    if (seg) {
      const up = next >= st.level
      row.salesSum = Math.round(
        up ? Math.max(seg.minSalesSum, st.sales) : Math.min(seg.minSalesSum, st.sales)
      )
      row.standing = up
        ? Math.max(seg.minStanding, st.standing)
        : Math.min(seg.minStanding, st.standing)
    }
    row.unlocked = next > 1 || st.unlocked
  }

  /** 等级步进：复刻 LoyaltyLevel setter（捕获 → 钳位 → 联动补值） */
  function onStep(row: PhxTraderRow, delta: number): void {
    if (!row.unlocked) return
    const st = captureStart(row)
    const next = Math.min(row.maxLevel, Math.max(1, row.loyaltyLevel + delta))
    if (next === row.loyaltyLevel) return
    row.loyaltyLevel = next
    applyLevelLink(row, next, st)
    syncTexts()
  }

  /** 手改声望/销售后反推等级（SetLevel 语义：最末匹配段胜出，无匹配不变，不动输入值） */
  function setLevelFromBase(row: PhxTraderRow): void {
    let matched = 0
    for (let i = 0; i < row.levels.length; i++) {
      if (row.standing >= row.levels[i].minStanding && row.salesSum >= row.levels[i].minSalesSum) {
        matched = i + 1
      }
    }
    if (matched > 0 && matched !== row.loyaltyLevel) row.loyaltyLevel = matched
  }

  function onStandingInput(e: Event, row: PhxTraderRow): void {
    const v = parseFloat((e.currentTarget as HTMLInputElement).value)
    if (!Number.isNaN(v)) row.standing = v
    standingTexts[row.id] = fmt2(row.standing)
    setLevelFromBase(row)
  }

  function onSalesInput(e: Event, row: PhxTraderRow): void {
    const digits = (e.currentTarget as HTMLInputElement).value.replace(/[^\d]/g, '')
    const v = Number(digits)
    if (digits !== '' && Number.isFinite(v)) row.salesSum = Math.trunc(v)
    salesTexts[row.id] = fmtSales(row.salesSum)
    setLevelFromBase(row)
  }

  /** 跳蚤评分：可负，下限 −20 */
  function onRatingInput(e: Event): void {
    const v = parseFloat((e.currentTarget as HTMLInputElement).value)
    if (!Number.isNaN(v)) rating = Math.max(-20, v)
    ratingText = fmt2(rating)
  }

  /** 全部拉满：逐商人单跳到各自上限（联动自动补顶级下限并放行） */
  function onMaxAll(): void {
    for (const row of traders) onStep(row, row.maxLevel - row.loyaltyLevel)
  }

  /** 全量刷新：当前档案 → 商人全量数据；成功后清空联动快照 */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    if (!currentId) {
      traders = []
      return
    }
    const data = await phxTraders.load(currentId)
    if (!data) {
      traders = []
      return
    }
    traders = data.traders
    rating = data.ragfair.rating
    starts = {}
    loaded = true
    syncTexts()
  }

  /** 保存修改：写回前主进程强制自动备份（领域铁律）；等级钳位、销售取非负整 */
  async function onSave(): Promise<void> {
    if (!loaded || saving) return
    saving = true
    try {
      const payload: PhxTradersSaveData = {
        traders: traders.map((r) => ({
          id: r.id,
          loyaltyLevel: Math.min(r.maxLevel, Math.max(1, r.loyaltyLevel)),
          salesSum: Math.max(0, Math.trunc(r.salesSum)),
          standing: r.standing,
          unlocked: r.unlocked
        })),
        ragfair: rating
      }
      await phxTraders.save(currentId, payload)
      toast.success(t('traders.toast.saved'))
    } catch {
      toast.error(t('traders.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 显示名：已知商人走本地化，未知用原名回退 */
  function displayName(row: PhxTraderRow): string {
    return knownOf(row.id) ? t(knownOf(row.id)!.key) : row.nickname || t('traders.name.unknown')
  }

  /** 副行英文名：已知商人固定；未知显示 ID 短码避免重复 */
  function subName(row: PhxTraderRow): string {
    const k = knownOf(row.id)
    if (k) return k.en
    return row.id.slice(0, 8)
  }

  /** 头像字：已知用稿图原值，未知取首字符大写 */
  function avOf(row: PhxTraderRow): string {
    const k = knownOf(row.id)
    if (k) return k.av
    const n = (row.nickname || t('traders.name.unknown')).trim()
    return (n.charAt(0) || '？').toUpperCase()
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新商人页（Rust 侧 emit；本页卸载时退订）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <header class="main-head">
    <div>
      <div class="large-title">{t('nav.traders')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button class="btn" type="button" onclick={onMaxAll} disabled={!loaded || saving}>
        {t('traders.act.max')}
      </button>
      <button class="btn primary" type="button" onclick={() => void onSave()} disabled={!loaded || saving}>
        {t('char.act.save')}
      </button>
    </div>
  </header>

  <div class="scroll phx-hide-scroll">
    {#if loaded}
      <div class="grid">
        {#each sorted as row (row.id)}
          <section class="tcard" class:locked={!row.unlocked}>
            <div class="tcard-head">
              <span class="tav">{avOf(row)}</span>
              <div class="tinfo">
                <div class="nm">{displayName(row)}</div>
                <div class="en">{subName(row)}</div>
              </div>
              <PhxSwitch
                size="lg"
                bind:checked={row.unlocked}
                label={t('traders.aria.unlock', { name: displayName(row) })}
              />
            </div>
            <div class="trow">
              <span class="k">{t('traders.f.level')}</span>
              <div class="stepper">
                <button type="button" aria-label="-" onclick={() => onStep(row, -1)} disabled={!row.unlocked}>−</button>
                <span class="val">{row.loyaltyLevel}</span>
                <button type="button" aria-label="+" onclick={() => onStep(row, 1)} disabled={!row.unlocked}>+</button>
              </div>
              <span class="rng">{`1 – ${row.maxLevel}`}</span>
            </div>
            <div class="trow">
              <span class="k">{t('traders.f.standing')}</span>
              <input
                class="mval"
                value={standingTexts[row.id]}
                oninput={(e) => onStandingInput(e, row)}
                disabled={!row.unlocked}
              />
            </div>
            <div class="trow">
              <span class="k">{t('traders.f.sales')}</span>
              <input
                class="mval"
                value={salesTexts[row.id]}
                oninput={(e) => onSalesInput(e, row)}
                disabled={!row.unlocked}
              />
            </div>
          </section>
        {/each}

        <section class="tcard">
          <div class="tcard-head">
            <span class="tav">{t('traders.av.ragfair')}</span>
            <div class="tinfo">
              <div class="nm">{t('traders.name.ragfair')}</div>
              <div class="en">Ragfair</div>
            </div>
          </div>
          <div class="trow">
            <span class="k">{t('traders.f.rating')}</span>
            <input class="mval" value={ratingText} oninput={onRatingInput} />
          </div>
          <div class="hint">{t('traders.hint.ragfair')}</div>
        </section>
      </div>
    {:else}
      <div class="empty">{t('traders.empty.none')}</div>
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
  }
  .large-title {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.4px;
    line-height: 1.15;
  }
  .head-meta {
    margin-top: 6px;
    font-size: 13px;
    color: var(--phx-text-2);
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 2px;
  }
  .btn {
    height: 32px;
    padding: 0 15px;
    border: none;
    border-radius: var(--phx-r-btn);
    font-size: 13px;
    font-weight: 600;
    font-family: inherit;
    cursor: pointer;
  }
  .btn.primary {
    background: var(--phx-accent);
    color: #fff;
  }
  .btn.primary:not(:disabled):hover {
    filter: brightness(1.12);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px;
  }
  .empty {
    margin-top: 18px;
    padding: 60px 0;
    text-align: center;
    font-size: 13px;
    color: var(--phx-text-3);
  }

  /* ---- 商人卡（稿图 04 专属） ---- */
  .grid {
    margin-top: 18px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
  }
  .tcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 12px 14px;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tcard-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 8px;
  }
  .tav {
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 50%;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 15px;
    font-weight: 700;
  }
  .tinfo {
    flex: 1;
    min-width: 0;
  }
  .tinfo .nm {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text);
  }
  .tinfo .en {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    margin-top: 1px;
  }
  .trow {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
  }
  .trow + .trow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .k {
    flex: none;
    width: 60px;
    font-size: 13px;
    color: var(--phx-text-2);
  }
  .mval {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 8px;
    text-align: right;
    border: none;
    outline: none;
    border-radius: 7px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    font-family: var(--phx-font-mono);
    font-size: 13px;
    transition: box-shadow 0.15s;
  }
  .mval:focus {
    box-shadow: 0 0 0 3px var(--phx-accent-dim);
  }
  .stepper {
    margin-left: auto;
    display: flex;
    width: 94px;
    height: 29px;
    background: var(--phx-fill-tertiary);
    border-radius: 8px;
    overflow: hidden;
  }
  .stepper button {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--phx-text);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }
  .stepper button:disabled {
    cursor: default;
  }
  .stepper .val {
    min-width: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--phx-font-mono);
    font-size: 14px;
    border-left: 0.5px solid var(--phx-sep-strong);
    border-right: 0.5px solid var(--phx-sep-strong);
  }
  .stepper button:active {
    background: var(--phx-fill-quaternary);
  }
  .rng {
    flex: none;
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
  }
  .tcard.locked .mval,
  .tcard.locked .stepper {
    opacity: 0.35;
    pointer-events: none;
  }
  .hint {
    padding: 8px 0 2px;
    font-size: 11px;
    color: var(--phx-text-3);
  }
</style>
