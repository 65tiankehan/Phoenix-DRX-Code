<script lang="ts">
  // 02b 首页：hero 单行 + 4 列便当网格（六路数据 tile）+ 四组入口，按 docs/prototypes/02b 稿图逐项落地
  import { onMount } from 'svelte'
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxProfile,
    phxIssues,
    phxSettings,
    phxQuests,
    phxHideout,
    phxStash,
    phxTraders,
    phxSkills,
    phxHealth,
    type PhxProfileHero,
    type PhxProfileMeta,
    type PhxIssueRow,
    type PhxQuestsData,
    type PhxHideoutData,
    type PhxStashData,
    type PhxTradersData,
    type PhxSkillsData,
    type PhxHealthData,
    type PhxHealthSide
  } from '$lib/ipc'
  import PhxDropdown from '$lib/components/PhxDropdown.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxIssueSheet from '$lib/components/PhxIssueSheet.svelte'
  import PhxHomeFeed from '$lib/components/PhxHomeFeed.svelte'

  /** 骨架镜像占位行序号（boot 层内部件用） */
  const skRows = (n: number) => [...Array(n).keys()]

  /** 入口图标：02 稿同款 svg inner markup（wrapper 统一由模板提供） */
  const ICON = {
    info: '<circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/>',
    skills: '<path d="M12 3l2.2 6.8L21 12l-6.8 2.2L12 21l-2.2-6.8L3 12l6.8-2.2Z"/>',
    mastery: '<circle cx="12" cy="12" r="7"/><path d="M12 3v4M12 17v4M3 12h4M17 12h4"/>',
    health:
      '<path d="M12 20s-7-4.5-7-9.5C5 7.5 7 6 9 6c1.4 0 2.4.7 3 1.7C12.6 6.7 13.6 6 15 6c2 0 4 1.5 4 4.5 0 5-7 9.5-7 9.5Z"/>',
    clothes:
      '<path d="M8.5 4 4 7l2 3 2-1v11h8V9l2 1 2-3-4.5-3c-1 1.4-2 2-3.5 2s-2.5-.6-3.5-2Z"/>',
    scav:
      '<circle cx="9" cy="8.5" r="3"/><path d="M3.5 19c1-3 3-4.5 5.5-4.5s4.5 1.5 5.5 4.5"/><path d="M15.5 5.6a3 3 0 1 1 1.5 5.7"/><path d="M16.5 14.8c2.2.5 3.7 1.9 4.4 4.2"/>',
    traders: '<path d="M6 8h12l-1 12H7L6 8Z"/><path d="M9 8V6.5a3 3 0 0 1 6 0V8"/>',
    stash:
      '<path d="M3.5 8.5 12 4l8.5 4.5v7L12 20l-8.5-4.5v-7Z"/><path d="M3.5 8.5 12 13l8.5-4.5M12 13v7"/>',
    pockets:
      '<rect x="3.5" y="6" width="17" height="13" rx="2.5"/><path d="M15.5 11.5h5v3h-5a1.5 1.5 0 1 1 0-3Z"/>',
    encyclopedia:
      '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/>',
    inspect: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
    dataDir: '<path d="M3.5 7.5c0-1.1.9-2 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V7.5Z"/>',
    issues: '<path d="M12 4 2.5 20h19L12 4Z"/><path d="M12 10v4M12 16.8v.2"/>',
    backup:
      '<rect x="4" y="5" width="16" height="4" rx="1"/><path d="M6 9v9a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V9"/><path d="M12 12v4M10 14l2 2 2-2"/>',
    settings:
      '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1"/>',
    quests:
      '<rect x="5" y="4" width="14" height="17" rx="2"/><path d="M9 4.5V3h6v1.5"/><path d="m9.5 13 2 2 3.5-3.5"/>',
    hideout: '<path d="M3.5 19h17L12 5 3.5 19Z"/><path d="m9.5 19 2.5-5 2.5 5"/>',
    presets:
      '<path d="M5 6h14M5 12h14M5 18h14"/><circle cx="9" cy="6" r="1.8"/><circle cx="15" cy="12" r="1.8"/><circle cx="8" cy="18" r="1.8"/>'
  } as const

  /** 入口可达路由（resolve 强类型要求字面量） */
  type EntryRoute =
    | '/character'
    | '/skills'
    | '/health'
    | '/traders'
    | '/stash'
    | '/inspect'
    | '/issues'
    | '/backup'
    | '/settings'
    | '/quests'
    | '/hideout'
    | '/presets'
    | '/encyclopedia'

  interface DashEntry {
    key: keyof typeof ICON
    to?: EntryRoute
    /** 无落地屏的入口：点击提示将在后续版本提供 */
    soon?: boolean
  }

  /** 02b 入口四组：每组一个 2 列 chip 行 */
  const ENTRY_GROUPS: Array<{ titleKey: MessageKey; items: DashEntry[] }> = [
    {
      titleKey: 'home.panel.character',
      items: [
        { key: 'info', to: '/character' },
        { key: 'skills', to: '/skills' },
        { key: 'mastery', to: '/skills' },
        { key: 'health', to: '/health' },
        { key: 'clothes', to: '/character' },
        { key: 'scav', to: '/character' }
      ]
    },
    {
      titleKey: 'home.panel.economy',
      items: [
        { key: 'traders', to: '/traders' },
        { key: 'stash', to: '/stash' },
        { key: 'pockets', soon: true },
        { key: 'encyclopedia', to: '/encyclopedia' },
        { key: 'inspect', to: '/inspect' },
        { key: 'dataDir' }
      ]
    },
    {
      titleKey: 'home.panel.progress',
      items: [
        { key: 'quests', to: '/quests' },
        { key: 'hideout', to: '/hideout' },
        { key: 'presets', to: '/presets' }
      ]
    },
    {
      titleKey: 'home.panel.maintain',
      items: [
        // 问题检测红徽章：计数由 refresh() 实算（issues_scan），0 时隐藏（02 稿注 6）
        { key: 'issues', to: '/issues' },
        { key: 'backup', to: '/backup' },
        { key: 'settings', to: '/settings' }
      ]
    }
  ]

  let profiles = $state<PhxProfileMeta[]>([])
  let currentId = $state('')
  let hero = $state<PhxProfileHero | null>(null)
  let saving = $state(false)
  let booting = $state(true)
  // 问题检测实算计数（02 稿注 6：红徽章，0 时隐藏）
  let issueCount = $state(0)
  // 保存拦截弹窗（13 稿）：ask 偏好下 scan 出问题时弹出
  let sheetOpen = $state(false)
  let sheetIssues = $state<PhxIssueRow[]>([])
  // 02b 六路 tile 数据（单路失败置 null，对应 tile 显示空态「—」）
  let quests = $state<PhxQuestsData | null>(null)
  let hideout = $state<PhxHideoutData | null>(null)
  let stash = $state<PhxStashData | null>(null)
  let traders = $state<PhxTradersData | null>(null)
  let skills = $state<PhxSkillsData | null>(null)
  let health = $state<PhxHealthData | null>(null)

  const ddOptions = $derived(profiles.map((p) => ({ value: p.id, label: p.id })))

  const metaText = $derived.by(() => {
    if (!hero) return t('home.meta.none')
    const v = hero.gameVersion
    return v ? t('home.meta.loaded', { v }) : t('home.meta.loadedPlain')
  })

  const levelText = $derived.by(() => {
    if (!hero) return ''
    return hero.side
      ? t('home.hero.level', { n: hero.level, side: hero.side })
      : t('home.hero.levelOnly', { n: hero.level })
  })

  async function loadHeroData(): Promise<void> {
    hero = currentId ? await phxProfile.load(currentId) : null
  }

  // —— 任务五段聚合（前端口径：Success→已完成 / AvailableForFinish→可交付 /
  //    Started→进行中 / AvailableForStart→可开始 / 其余→锁定，总和恒等 rows.length）——
  const qData = $derived.by(() => {
    const rows = quests?.rows ?? []
    const total = rows.length
    let done = 0
    let fin = 0
    let run = 0
    let ava = 0
    let lock = 0
    for (const r of rows) {
      if (r.status === 'Success') done++
      else if (r.status === 'AvailableForFinish') fin++
      else if (r.status === 'Started') run++
      else if (r.status === 'AvailableForStart') ava++
      else lock++
    }
    // 零段跳过：避免分段条 gap 画出空气隙
    const segs =
      total > 0
        ? [
            { cls: 'q-done', w: (done / total) * 100, n: done },
            { cls: 'q-fin', w: (fin / total) * 100, n: fin },
            { cls: 'q-run', w: (run / total) * 100, n: run },
            { cls: 'q-ava', w: (ava / total) * 100, n: ava },
            { cls: 'q-lock', w: (lock / total) * 100, n: lock }
          ].filter((s) => s.n > 0)
        : []
    // 可交付引导行：最早一条可交付任务名（rows 顺序即存档内顺序）
    const finFirst = rows.find((r) => r.status === 'AvailableForFinish')?.questName ?? ''
    return { total, done, fin, run, ava, lock, segs, finFirst }
  })

  // 藏身处前 6 行（pips 预计算：'on' 已点亮 / 'max' 满级 / '' 未点亮）
  const hideoutRows = $derived.by(() => {
    if (!hideout) return []
    return hideout.areas.slice(0, 6).map((a) => ({
      name: a.name,
      level: a.level,
      maxLevel: a.maxLevel,
      pips: Array.from({ length: a.maxLevel }, (_, k) =>
        a.level >= a.maxLevel ? 'max' : k < a.level ? 'on' : ''
      )
    }))
  })
  const hideoutMaxed = $derived.by(() => {
    if (!hideout) return { done: 0, total: 0 }
    return {
      total: hideout.areas.length,
      done: hideout.areas.filter((a) => a.maxLevel > 0 && a.level >= a.maxLevel).length
    }
  })
  // 下一处可升级：第一个未满级区域名（t-foot 右项引导）
  const hideoutNext = $derived.by(() => {
    if (!hideout) return ''
    return hideout.areas.find((a) => a.maxLevel > 0 && a.level < a.maxLevel)?.name ?? ''
  })

  // 仓库（02b 稿注 ③：无上限数据，不画占用率；25 稿补身上装备/口袋两行）
  const stashStats = $derived.by(() => {
    if (!stash) return null
    return {
      count: stash.items.length,
      bonus: stash.bonusRows,
      weapons: stash.items.filter((i) => i.isWeapon).length,
      modded: stash.items.filter((i) => i.modded).length,
      // 身上装备：装备槽非空计数 / 总槽数
      equip: stash.equipment.filter((e) => e.item).length,
      equipTotal: stash.equipment.length,
      pockets: stash.pockets.length
    }
  })

  // 商人前 5 行（pips 满级点亮全段）
  const traderRows = $derived.by(() => {
    if (!traders) return []
    return traders.traders.slice(0, 5).map((tr) => ({
      nickname: tr.nickname,
      level: tr.loyaltyLevel,
      maxLevel: tr.maxLevel,
      pips: Array.from({ length: tr.maxLevel }, (_, k) =>
        tr.loyaltyLevel >= tr.maxLevel ? 'max' : k < tr.loyaltyLevel ? 'on' : ''
      )
    }))
  })
  const traderMaxed = $derived.by(() => {
    if (!traders) return { done: 0, total: 0 }
    return {
      total: traders.traders.length,
      done: traders.traders.filter((x) => x.maxLevel > 0 && x.loyaltyLevel >= x.maxLevel).length
    }
  })

  // 技能 top4（25 稿：top3 → top4；过滤 bot 内部技能；cap 防除零）
  const skillRows = $derived.by(() => {
    if (!skills) return []
    return skills.pmc.common
      .filter((s) => !s.bot)
      .map((s) => ({
        name: s.name,
        pct: s.cap > 0 ? Math.min(100, Math.round((s.progress / s.cap) * 100)) : 0
      }))
      .sort((a, b) => b.pct - a.pct)
      .slice(0, 4)
  })
  const skillAvg = $derived.by(() => {
    if (!skills) return null
    const rows = skills.pmc.common.filter((s) => !s.bot && s.cap > 0)
    if (rows.length === 0) return 0
    const sum = rows.reduce((acc, s) => acc + Math.min(100, (s.progress / s.cap) * 100), 0)
    return Math.round(sum / rows.length)
  })

  // 健康 PMC 九部位（左右腿分开；<15% 红 / <35% 黄）
  const hpRows = $derived.by(() => {
    const p = health?.pmc
    if (!p) return []
    const defs: Array<{ key: MessageKey; part: PhxHealthSide['head'] }> = [
      { key: 'char.health.head', part: p.head },
      { key: 'char.health.chest', part: p.chest },
      { key: 'char.health.stomach', part: p.stomach },
      { key: 'char.health.leftArm', part: p.leftArm },
      { key: 'char.health.rightArm', part: p.rightArm },
      { key: 'char.health.leftLeg', part: p.leftLeg },
      { key: 'char.health.rightLeg', part: p.rightLeg },
      { key: 'char.health.hydration', part: p.hydration },
      { key: 'char.health.energy', part: p.energy }
    ]
    return defs.map(({ key, part }) => {
      const pct = part.maximum > 0 ? Math.round((part.current / part.maximum) * 100) : 0
      return {
        key,
        cur: part.current,
        max: part.maximum,
        pct,
        cls: pct < 15 ? 'bad' : pct < 35 ? 'warn' : ''
      }
    })
  })
  // 健康 t-foot 异常摘要：血量/水分/能量偏低的部位（25 稿新增）
  const hpIssues = $derived(hpRows.filter((h) => h.cls !== ''))

  async function loadTiles(): Promise<void> {
    const id = currentId
    if (!id) {
      quests = null
      hideout = null
      stash = null
      traders = null
      skills = null
      health = null
      return
    }
    // 六路并发（allSettled：单路失败置 null，不拖垮其余 tile；解包变量避开 i18n 的 t）
    const [qRes, hRes, sRes, trRes, skRes, hpRes] = await Promise.allSettled([
      phxQuests.load(id),
      phxHideout.load(id),
      phxStash.load(id),
      phxTraders.load(id),
      phxSkills.load(id),
      phxHealth.load(id)
    ])
    // 拉取期间切换档案：丢弃旧数据
    if (id !== currentId) return
    quests = qRes.status === 'fulfilled' ? qRes.value : null
    hideout = hRes.status === 'fulfilled' ? hRes.value : null
    stash = sRes.status === 'fulfilled' ? sRes.value : null
    traders = trRes.status === 'fulfilled' ? trRes.value : null
    skills = skRes.status === 'fulfilled' ? skRes.value : null
    health = hpRes.status === 'fulfilled' ? hpRes.value : null
  }

  /** 全量刷新：列表 → 当前选择（失效回退首个）→ 概览数据 → 问题计数 → 六路 tile */
  async function refresh(): Promise<void> {
    booting = true
    const list = await phxProfile.list()
    profiles = list
    let id = await phxProfile.current()
    if (id && !list.some((p) => p.id === id)) id = null
    if (!id && list.length > 0) id = list[0].id
    if (id && id !== currentId) await phxProfile.use(id)
    currentId = id ?? ''
    await loadHeroData()
    issueCount = currentId ? (await phxIssues.scan(currentId)).length : 0
    await loadTiles()
    booting = false
  }

  async function onPick(id: string): Promise<void> {
    if (id === currentId) return
    currentId = id
    await phxProfile.use(id)
    await loadHeroData()
    await loadTiles()
  }

  /**
   * 保存链（13 稿注 3）：先自动备份（Rust 侧强制）→ 扫描问题 → 按三态偏好处理。
   * ask 弹窗期间 saving 持续为真，弹窗结束（resolve）后继续或放弃。
   */
  async function onSave(): Promise<void> {
    if (!hero || saving) return
    saving = true
    try {
      const settings = await phxSettings.get()
      const issues = await phxIssues.scan(hero.id)
      issueCount = issues.length
      if (issues.length === 0 || settings.issueHandling === 'ignore') {
        await doSave()
        return
      }
      if (settings.issueHandling === 'fix') {
        await phxIssues.fix(
          hero.id,
          issues.map((i) => i.id)
        )
        issueCount = (await phxIssues.scan(hero.id)).length
        await doSave()
        return
      }
      // 询问：弹出问题弹窗（saving 保持为真，避免保存期间重复触发）
      sheetIssues = issues
      sheetOpen = true
    } catch {
      toast.error(t('home.toast.failed'))
      saving = false
    }
  }

  /** 写盘（Rust 侧先落备份）+ 成败反馈 */
  async function doSave(): Promise<void> {
    try {
      await phxProfile.save(hero!.id)
      toast.success(t('home.toast.saved'))
    } catch {
      toast.error(t('home.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 问题弹窗结束：save = 已忽略或全部处理完，继续写盘；cancel = 放弃本次保存 */
  function onSheetResolve(action: 'save' | 'cancel'): void {
    sheetOpen = false
    if (action === 'cancel' || !hero) {
      saving = false
      return
    }
    void (async () => {
      await doSave()
      try {
        issueCount = (await phxIssues.scan(hero!.id)).length
      } catch {
        // 重扫失败不影响保存结果
      }
    })()
  }

  function onEntry(entry: DashEntry): void {
    if (entry.soon) {
      toast.warning(t('home.toast.soon'))
      return
    }
    // 数据目录无落地屏：直接打开系统资源管理器（能力同设置页）
    if (entry.key === 'dataDir') {
      void phxSettings.openDataDir().then(() => toast.success(t('settings.toast.opened')))
      return
    }
    if (entry.to) void goto(resolve(entry.to))
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新首页（Rust 侧 emit；本页卸载时退订）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <div class="main-head">
    <div>
      <div class="large-title">{t('nav.home')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button type="button" class="btn secondary" onclick={() => void goto(resolve('/server-select'))}>
        {t('home.act.back')}
      </button>
      <button type="button" class="btn primary" disabled={!hero || saving} onclick={() => void onSave()}>
        {t('home.act.save')}
      </button>
    </div>
  </div>

  <div class="scroll phx-hide-scroll">
    <!-- 骨架层：镜像便当网格真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="bento">
        <!-- hero（span4） -->
        <section class="hero">
          <i class="phx-sk av"></i>
          <div class="sk-stack">
            <i class="phx-sk slim" style="width:84px"></i>
            <i class="phx-sk" style="width:176px"></i>
          </div>
          <i class="phx-sk slim" style="width:44px;margin-left:6px"></i>
          <div class="hero-stats">
            <div class="stat"><i class="phx-sk slim" style="width:40px"></i><i class="phx-sk" style="width:56px"></i></div>
            <div class="stat"><i class="phx-sk slim" style="width:40px"></i><i class="phx-sk" style="width:56px"></i></div>
            <div class="stat"><i class="phx-sk slim" style="width:40px"></i><i class="phx-sk" style="width:56px"></i></div>
          </div>
          <i class="phx-sk hr-badge"></i>
        </section>
        <!-- 动态条（span4）：34px 窄行占位 -->
        <div class="feed sk-feed">
          <i class="phx-sk slim" style="width:34px;height:18px"></i>
          <i class="phx-sk slim grow"></i>
          <i class="phx-sk slim" style="width:44px"></i>
        </div>
        <!-- 任务（span2）：大数 + 五段分段条 + 图例 -->
        <section class="tile span2">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          <div class="q-big"><i class="phx-sk" style="width:54px"></i><i class="phx-sk slim" style="width:92px"></i></div>
          <div class="q-bar">
            <i class="phx-sk slot" style="width:30%"></i>
            <i class="phx-sk slot" style="width:16%"></i>
            <i class="phx-sk slot" style="width:20%"></i>
            <i class="phx-sk slot" style="width:15%"></i>
            <i class="phx-sk slot" style="width:19%"></i>
          </div>
          <div class="q-legend">
            <i class="phx-sk slim" style="width:58px"></i>
            <i class="phx-sk slim" style="width:70px"></i>
            <i class="phx-sk slim" style="width:52px"></i>
            <i class="phx-sk slim" style="width:66px"></i>
            <i class="phx-sk slim" style="width:48px"></i>
          </div>
        </section>
        <!-- 藏身处：6 行区域 + 满级计数 -->
        <section class="tile">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          {#each skRows(6) as k (k)}
            <div class="h-row">
              <i class="phx-sk slim" style="width:70px"></i>
              <span class="pips">
                {#each skRows(5) as p (p)}<i class="phx-sk pip"></i>{/each}
              </span>
              <i class="phx-sk slim" style="width:34px"></i>
            </div>
          {/each}
          <div class="t-foot"><i class="phx-sk slim" style="width:70px"></i><i class="phx-sk slim" style="width:44px"></i></div>
        </section>
        <!-- 仓库：总数 + 五行统计 -->
        <section class="tile">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          <div class="s-big"><i class="phx-sk" style="width:64px"></i><i class="phx-sk slim" style="width:22px"></i></div>
          <div class="s-rows">
            <div class="s-row"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:40px"></i></div>
            <div class="s-row"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:40px"></i></div>
            <div class="s-row"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:40px"></i></div>
            <div class="s-row"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:40px"></i></div>
            <div class="s-row"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:40px"></i></div>
          </div>
        </section>
        <!-- 商人：5 行忠诚度 -->
        <section class="tile">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          {#each skRows(5) as k (k)}
            <div class="tr-row">
              <i class="phx-sk slim" style="width:70px"></i>
              <span class="pips">
                {#each skRows(5) as p (p)}<i class="phx-sk pip"></i>{/each}
              </span>
              <i class="phx-sk slim" style="width:34px"></i>
            </div>
          {/each}
          <div class="t-foot"><i class="phx-sk slim" style="width:70px"></i><i class="phx-sk slim" style="width:44px"></i></div>
        </section>
        <!-- 技能：top4 进度条 -->
        <section class="tile">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          {#each skRows(4) as k (k)}
            <div class="sk-row">
              <div class="l"><i class="phx-sk slim grow"></i><i class="phx-sk slim" style="width:34px"></i></div>
              <div class="sk-bar"><i class="phx-sk slot" style="width:68%"></i></div>
            </div>
          {/each}
          <div class="t-foot"><i class="phx-sk slim" style="width:88px"></i><i class="phx-sk slim" style="width:36px"></i></div>
        </section>
        <!-- 健康（span2）：九部位两列 -->
        <section class="tile span2">
          <div class="t-head"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:64px"></i></div>
          <div class="hp-grid">
            {#each skRows(9) as k (k)}
              <div class="hp-row">
                <i class="phx-sk slim" style="width:56px"></i>
                <div class="hp-bar"><i class="phx-sk slot" style="width:64%"></i></div>
                <i class="phx-sk slim" style="width:44px"></i>
              </div>
            {/each}
          </div>
        </section>
        <!-- 入口（span4）：四组 2×2 chip -->
        <section class="tile entries">
          <div class="en-grid">
            {#each [6, 6, 3, 3] as count, g (g)}
              <div class="en-group">
                <div class="en-title"><i class="phx-sk slim" style="width:76px"></i></div>
                <div class="en-chips">
                  {#each skRows(count) as k (k)}
                    <span class="chip"><i class="phx-sk ic"></i><i class="phx-sk slim" style="width:48px"></i></span>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </section>
      </div>
    </div>

    {#if !booting}
    <div class="bento phx-fade-in">
      <!-- hero（span4）：单行概览，不可点；徽章两态见 02b 稿注 ① -->
      <section class="hero">
        <div class="avatar">
          <PhxIcon markup={ICON.info} />
        </div>
        <div>
          <div class="hr-label">{t('home.hero.label')}</div>
          <div class="hr-id">
            {#if profiles.length > 0}
              <span class="hero-dd">
                <PhxDropdown
                  value={currentId}
                  options={ddOptions}
                  label={t('home.hero.label')}
                  bare
                  onchange={(v) => void onPick(v)}
                />
              </span>
            {:else}
              <span class="hr-id-text">{t('home.empty.noprofile')}</span>
            {/if}
          </div>
        </div>
        {#if hero}
          <div class="hr-lvl">{levelText}</div>
          <div class="hero-stats">
            <div class="stat">
              <div class="k">{t('home.hero.rub')}</div>
              <div class="v">{hero.rub ?? '—'}</div>
            </div>
            <div class="stat">
              <div class="k">{t('home.hero.usd')}</div>
              <div class="v">{hero.usd ?? '—'}</div>
            </div>
            <div class="stat">
              <div class="k">{t('home.hero.eur')}</div>
              <div class="v">{hero.eur ?? '—'}</div>
            </div>
          </div>
          {#if issueCount === 0}
            <span class="hero-badge ok">
              <i class="dot"></i>{t('home.badge.ok')}
            </span>
          {:else}
            <button type="button" class="hero-badge" onclick={() => void goto(resolve('/issues'))}>
              <i class="dot"></i>{t('home.badge.issues', { n: issueCount })}
            </button>
          {/if}
        {/if}
      </section>

      <!-- 动态条（25 稿，span4）：轮播 公告/小贴士/赞助；全空自动不渲染 -->
      <PhxHomeFeed />

      <!-- 任务（span2）：五段分段条（前端口径聚合） -->
      <button type="button" class="tile span2" onclick={() => void goto(resolve('/quests'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.quests} /></span>
          <span class="t-name">{t('home.tile.quests')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if qData.total > 0}
          <div class="q-big">
            <span class="n">{qData.done}</span>
            <span class="d">{t('home.tile.qtotal', { total: qData.total })}</span>
          </div>
          <div class="q-bar">
            {#each qData.segs as s (s.cls)}
              <i class={s.cls} style={`width:${s.w}%`}></i>
            {/each}
          </div>
          <div class="q-legend">
            <span><i class="q-done"></i>{t('quests.st.Success')} {qData.done}</span>
            <span><i class="q-fin"></i>{t('quests.st.AvailableForFinish')} {qData.fin}</span>
            <span><i class="q-run"></i>{t('quests.st.Started')} {qData.run}</span>
            <span><i class="q-ava"></i>{t('quests.st.AvailableForStart')} {qData.ava}</span>
            <span><i class="q-lock"></i>{t('quests.st.Locked')} {qData.lock}</span>
          </div>
          {#if qData.fin > 0 && qData.finFirst}
            <div class="q-next">
              <span class="tag">{t('home.tile.qfin', { n: qData.fin })}</span>
              <span class="txt">{t('home.tile.qnext')}<b>{qData.finFirst}</b></span>
              <span class="arr">›</span>
            </div>
          {/if}
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 藏身处：前 6 行区域 + 满级计数 -->
      <button type="button" class="tile" onclick={() => void goto(resolve('/hideout'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.hideout} /></span>
          <span class="t-name">{t('home.tile.hideout')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if hideoutRows.length > 0}
          {#each hideoutRows as h (h.name)}
            <div class="h-row">
              <span class="n">{h.name}</span>
              <span class="pips">
                {#each h.pips as p, k (k)}
                  <i class={p}></i>
                {/each}
              </span>
              <span class="lv">{h.level}/{h.maxLevel}</span>
            </div>
          {/each}
          <div class="t-foot">
            <span><span class:ok={hideoutMaxed.done > 0}>{t('home.maxed')}</span> <b class:ok={hideoutMaxed.done > 0}>{hideoutMaxed.done} / {hideoutMaxed.total}</b></span>
            {#if hideoutNext}
              <span>{t('home.hideout.next')} <b>{hideoutNext}</b></span>
            {/if}
          </div>
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 仓库：总数 + 五行统计（02b 稿注 ③：无上限数据，不画占用率；25 稿补身上装备/口袋） -->
      <button type="button" class="tile" onclick={() => void goto(resolve('/stash'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.stash} /></span>
          <span class="t-name">{t('home.tile.stash')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if stashStats}
          <div class="s-big">
            <span class="n">{stashStats.count}</span>
            <span class="u">{t('home.stash.unit')}</span>
          </div>
          <div class="s-rows">
            <div class="s-row"><span>{t('home.stash.bonus')}</span><b class="hl">+{stashStats.bonus}</b></div>
            <div class="s-row"><span>{t('home.stash.weapons')}</span><b>{stashStats.weapons}</b></div>
            <div class="s-row"><span>{t('home.stash.modded')}</span><b>{stashStats.modded}</b></div>
            <div class="s-row"><span>{t('home.stash.equip')}</span><b>{stashStats.equip} / {stashStats.equipTotal}</b></div>
            <div class="s-row"><span>{t('home.stash.pockets')}</span><b>{stashStats.pockets} {t('home.stash.unitShort')}</b></div>
          </div>
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 商人：前 5 行忠诚度 -->
      <button type="button" class="tile" onclick={() => void goto(resolve('/traders'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.traders} /></span>
          <span class="t-name">{t('home.tile.traders')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if traderRows.length > 0}
          {#each traderRows as tr (tr.nickname)}
            <div class="tr-row">
              <span class="n">{tr.nickname}</span>
              <span class="pips">
                {#each tr.pips as p, k (k)}
                  <i class={p}></i>
                {/each}
              </span>
              <span class="lv">{tr.level}/{tr.maxLevel}</span>
            </div>
          {/each}
          <div class="t-foot">
            <span class:ok={traderMaxed.done > 0}>{t('home.maxed')}</span>
            <b class:ok={traderMaxed.done > 0}>{traderMaxed.done} / {traderMaxed.total}</b>
          </div>
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 技能：top3 + PMC 平均进度 -->
      <button type="button" class="tile" onclick={() => void goto(resolve('/skills'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.skills} /></span>
          <span class="t-name">{t('home.tile.skills')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if skillRows.length > 0}
          {#each skillRows as sk (sk.name)}
            <div class="sk-row">
              <div class="l"><span>{sk.name}</span><b>{sk.pct}%</b></div>
              <div class="sk-bar"><i style={`width:${sk.pct}%`}></i></div>
            </div>
          {/each}
          <div class="t-foot">
            <span>{t('home.skills.avg')}</span>
            <b>{skillAvg}%</b>
          </div>
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 健康（span2）：PMC 九部位两列 -->
      <button type="button" class="tile span2" onclick={() => void goto(resolve('/health'))}>
        <div class="t-head">
          <span class="t-ic"><PhxIcon markup={ICON.health} /></span>
          <span class="t-name">{t('home.tile.health')}</span>
          <span class="t-chev">›</span>
        </div>
        {#if hpRows.length > 0}
          <div class="hp-grid">
            {#each hpRows as hp (hp.key)}
              <div class="hp-row">
                <span class="n">{t(hp.key)}</span>
                <div class="hp-bar"><i class={hp.cls} style={`width:${hp.pct}%`}></i></div>
                <span class="v">{hp.cur}/{hp.max}</span>
              </div>
            {/each}
          </div>
          {#if hpIssues.length > 0}
            <div class="t-foot">
              <span class="pair"><i class="wdot"></i>{t('home.health.low', { n: hpIssues.length })}</span>
              <b>{hpIssues.map((h) => `${t(h.key)} ${h.cur}/${h.max}`).join(' · ')}</b>
            </div>
          {/if}
        {:else}
          <div class="t-none">—</div>
        {/if}
      </button>

      <!-- 入口（span4）：四组 2×2 chip 行 -->
      <section class="tile entries">
        <div class="en-grid">
          {#each ENTRY_GROUPS as group (group.titleKey)}
            <div class="en-group">
              <div class="en-title">{t(group.titleKey)}</div>
              <div class="en-chips">
                {#each group.items as item (item.key)}
                  <button type="button" class="chip" onclick={() => onEntry(item)}>
                    <PhxIcon markup={ICON[item.key]} />
                    <span>{t(`home.item.${item.key}` as MessageKey)}</span>
                    {#if item.soon}<span class="soon">{t('home.soon')}</span>{/if}
                    {#if item.key === 'issues' && issueCount > 0}
                      <span class="cnt">{issueCount}</span>
                    {/if}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </section>
    </div>
    {/if}
  </div>

  <PhxIssueSheet
    open={sheetOpen}
    profileId={hero?.id ?? ''}
    issues={sheetIssues}
    onresolve={onSheetResolve}
  />
</div>

<style>
  /* 02b 稿图原值：页头 + 4 列便当网格（hero + 六路数据 tile + 入口组） */
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
    gap: 8px;
    padding-bottom: 4px;
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
  .btn.secondary {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .btn.secondary:hover {
    background: var(--phx-fill-quaternary);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 104px;
    position: relative; /* .phx-boot 骨架层的定位基准 */
  }

  /* 骨架层内部：与 .scroll 同 padding 镜像真实几何，占位条走全局 .phx-sk 原语 */
  .boot {
    padding: 0 24px 104px;
  }
  .boot .q-bar .phx-sk.slot {
    height: 10px;
    border-radius: 5px;
  }
  .boot .sk-bar .phx-sk.slot {
    height: 6px;
    border-radius: 3px;
  }
  .boot .hp-bar .phx-sk.slot {
    height: 5px;
    border-radius: 2.5px;
  }
  .sk-stack {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .hr-badge {
    width: 88px;
    height: 30px;
    border-radius: 15px;
  }

  /* —— 便当网格与 tile 通用 —— */
  .bento {
    margin-top: 16px;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .tile {
    /* button 默认把内容垂直居中，内容少的 tile（仓库）会整体下沉——显式改为贴顶堆叠 */
    display: flex;
    flex-direction: column;
    align-items: stretch;
    border: none;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    padding: 11px 14px 12px;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    font-size: 13px;
    color: var(--phx-text);
    transition: filter var(--phx-dur-fast) var(--phx-ease-out);
  }
  .tile:hover {
    filter: brightness(1.12);
  }
  .span2 {
    grid-column: span 2;
  }

  /* —— hero（span4，不可点）—— */
  .hero {
    grid-column: span 4;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    cursor: default;
  }
  .avatar {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .avatar :global(svg) {
    width: 19px;
    height: 19px;
  }
  .hr-label {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .hr-id {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 3px;
  }
  .hr-id-text {
    font-family: var(--phx-font-mono);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.2px;
  }
  /* bare 下拉的字体上下文：mono 15px 600，通过 font: inherit 传给触发钮 */
  .hero-dd {
    font-family: var(--phx-font-mono);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.2px;
    max-width: 100%;
  }
  .hero-dd :global(.dd-val) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hr-lvl {
    margin-left: 6px;
    font-size: 13px;
    color: var(--phx-text-2);
    flex: none;
  }
  .hero-stats {
    margin-left: 18px;
    padding-left: 18px;
    border-left: 0.5px solid var(--phx-sep-strong);
    flex: 1;
    display: flex;
  }
  .stat {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }
  .stat + .stat {
    border-left: 0.5px solid var(--phx-sep);
  }
  .stat .k {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .stat .v {
    font-family: var(--phx-font-mono);
    font-size: 16px;
    font-weight: 600;
  }
  .hero-badge {
    height: 30px;
    padding: 0 12px;
    border: none;
    border-radius: 15px;
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    flex: none;
    cursor: pointer;
    transition: filter var(--phx-dur-fast) var(--phx-ease-out);
  }
  .hero-badge:hover {
    filter: brightness(1.12);
  }
  .hero-badge.ok {
    background: var(--phx-green-dim);
    color: var(--phx-green);
    cursor: default;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }

  /* —— tile 头部 —— */
  .t-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .t-ic {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .t-ic :global(svg) {
    width: 13px;
    height: 13px;
  }
  .t-name {
    font-size: 13px;
    font-weight: 600;
  }
  .t-chev {
    margin-left: auto;
    font-size: 15px;
    line-height: 1;
    color: var(--phx-text-3);
  }
  .t-none {
    font-size: 12px;
    color: var(--phx-text-3);
  }

  /* —— 任务 —— */
  .q-big {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .q-big .n {
    font-family: var(--phx-font-mono);
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
    line-height: 1.1;
  }
  .q-big .d {
    font-size: 16px;
    color: var(--phx-text-3);
  }
  .q-bar {
    height: 10px;
    border-radius: 5px;
    overflow: hidden;
    margin: 10px 0 9px;
    display: flex;
    gap: 2px;
  }
  .q-bar i {
    display: block;
  }
  .q-done {
    background: var(--phx-green);
  }
  .q-fin {
    background: var(--phx-accent);
  }
  .q-run {
    background: var(--phx-accent-mid);
  }
  .q-ava {
    background: var(--phx-fill-secondary);
  }
  .q-lock {
    background: var(--phx-fill-tertiary);
  }
  .q-legend {
    display: flex;
    gap: 14px;
  }
  .q-legend span {
    font-size: 11px;
    color: var(--phx-text-2);
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .q-legend i {
    width: 6px;
    height: 6px;
    border-radius: 2px;
  }

  /* 可交付引导行（25 稿新增填充） */
  .q-next {
    margin-top: 9px;
    padding-top: 7px;
    border-top: 0.5px solid var(--phx-sep);
    font-size: 11px;
    color: var(--phx-text-2);
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .q-next .tag {
    flex: none;
    height: 17px;
    padding: 0 7px;
    border-radius: 5px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-size: 10px;
    font-weight: 700;
    display: flex;
    align-items: center;
  }
  .q-next .txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .q-next .txt b {
    color: var(--phx-text);
    font-weight: 600;
  }
  .q-next .arr {
    flex: none;
    color: var(--phx-text-3);
  }

  /* —— 藏身处/商人行 —— */
  .h-row {
    height: 23px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .h-row .n {
    width: 64px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pips {
    display: flex;
    gap: 3px;
    flex: 1;
  }
  .pips i {
    width: 14px;
    height: 5px;
    border-radius: 2.5px;
    background: var(--phx-fill-quaternary);
  }
  .pips i.on {
    background: var(--phx-accent);
  }
  .pips i.max {
    background: var(--phx-green);
  }
  .lv {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-2);
    flex: none;
  }
  .t-foot {
    margin-top: 7px;
    padding-top: 6px;
    border-top: 0.5px solid var(--phx-sep);
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    color: var(--phx-text-2);
  }
  .t-foot b {
    font-family: var(--phx-font-mono);
    font-weight: 600;
  }
  .t-foot b.ok {
    color: var(--phx-green);
  }
  /* 异常摘要行（25 稿新增填充）：偏低计数圆点 + 明细 */
  .t-foot .pair {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .wdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--phx-warn);
    flex: none;
  }
  .t-foot span.ok {
    color: var(--phx-green);
  }

  /* —— 商人 —— */
  .tr-row {
    height: 23px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tr-row .n {
    width: 70px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* —— 仓库 —— */
  .s-big {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .s-big .n {
    font-family: var(--phx-font-mono);
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
    line-height: 1.1;
  }
  .s-big .u {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .s-rows {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .s-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .s-row b {
    font-family: var(--phx-font-mono);
    font-weight: 600;
    color: var(--phx-text);
  }
  .s-row b.hl {
    color: var(--phx-accent);
  }

  /* —— 技能 —— */
  .sk-row {
    margin-top: 7px;
  }
  .sk-row .l {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    margin-bottom: 4px;
  }
  .sk-row .l b {
    font-family: var(--phx-font-mono);
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .sk-bar {
    height: 6px;
    border-radius: 3px;
    background: var(--phx-fill-quaternary);
    overflow: hidden;
  }
  .sk-bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--phx-accent);
  }

  /* —— 健康 —— */
  .hp-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    column-gap: 14px;
    row-gap: 6px;
  }
  .hp-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hp-row .n {
    width: 56px;
    font-size: 11px;
    color: var(--phx-text-2);
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hp-bar {
    flex: 1;
    height: 5px;
    border-radius: 2.5px;
    background: var(--phx-fill-quaternary);
    overflow: hidden;
  }
  .hp-bar i {
    display: block;
    height: 100%;
    border-radius: 2.5px;
    background: var(--phx-green);
  }
  .hp-bar i.warn {
    background: var(--phx-warn);
  }
  .hp-bar i.bad {
    background: var(--phx-danger);
  }
  .hp-row .v {
    width: 52px;
    font-family: var(--phx-font-mono);
    font-size: 10px;
    color: var(--phx-text-2);
    text-align: right;
    flex: none;
  }

  /* —— 入口 —— */
  .entries {
    grid-column: span 4;
    cursor: default;
  }
  /* 动态条骨架占位（34px 窄行，镜像 PhxHomeFeed 几何） */
  .sk-feed {
    grid-column: span 4;
    height: 34px;
    border-radius: 10px;
    background: var(--phx-surface);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
  }
  .en-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    column-gap: 20px;
    row-gap: 4px;
  }
  .en-title {
    font-size: 11px;
    color: var(--phx-text-2);
    padding: 3px 0 2px;
  }
  .en-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    height: 26px;
    padding: 0 10px;
    border: none;
    border-radius: 13px;
    background: transparent;
    color: var(--phx-text);
    font-size: 12px;
    font-family: inherit;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    /* 25 稿：chip 等宽 3/行铺满（组内 6 项 = 2 行，3 项 = 1 行） */
    flex: 1 1 calc(33.33% - 3px);
    white-space: nowrap;
  }
  .chip:hover {
    background: var(--phx-fill-tertiary);
  }
  .chip :global(svg) {
    width: 13px;
    height: 13px;
    color: var(--phx-text-2);
    flex: none;
  }
  .chip .soon {
    font-size: 9px;
    color: var(--phx-text-3);
    border: 0.5px solid var(--phx-sep-strong);
    border-radius: 6px;
    padding: 1px 4px;
  }
  .chip .cnt {
    min-width: 16px;
    height: 16px;
    border-radius: 8px;
    background: var(--phx-danger);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
  }
</style>
