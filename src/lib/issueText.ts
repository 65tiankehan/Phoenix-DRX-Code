// 存档问题文案组装：保存拦截弹窗（PhxIssueSheet）与问题清单页（/issues）共用
import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
import zhCN from '$lib/i18n/zh-CN.json'
import type { PhxIssueRow } from '$lib/ipc'

/** quest-status 的 requiredText 是状态枚举名，用任务状态词条本地化（缺词条时原样返回） */
export function statusText(name: string): string {
  const key = `quests.st.${name}`
  return key in zhCN ? t(key as MessageKey) : name
}

/** 按 kind 组装行文案（口径对齐 13 稿示意行） */
export function issueText(row: PhxIssueRow): string {
  switch (row.kind) {
    case 'dup':
      return t('issues.kind.dup', { side: row.side ?? '' })
    case 'trader-level':
      return t('issues.kind.traderLevel', {
        trader: row.traderName ?? '',
        level: row.requiredText
      })
    case 'quest-level':
      return t('issues.kind.questLevel', {
        quest: row.questName ?? '',
        trader: row.questTraderName ?? '',
        level: row.requiredText
      })
    case 'quest-status':
      return t('issues.kind.questStatus', {
        quest: row.questName ?? '',
        trader: row.questTraderName ?? '',
        target: row.targetQuestName ?? '',
        targetTrader: row.targetTraderName ?? '',
        status: statusText(row.requiredText)
      })
    case 'quest-loyalty':
      return t('issues.kind.questLoyalty', {
        quest: row.questName ?? '',
        trader: row.questTraderName ?? '',
        target: row.targetTraderName ?? '',
        level: row.requiredText
      })
    case 'quest-standing':
      return t('issues.kind.questStanding', {
        quest: row.questName ?? '',
        trader: row.questTraderName ?? '',
        target: row.targetTraderName ?? '',
        value: row.requiredText
      })
  }
}
