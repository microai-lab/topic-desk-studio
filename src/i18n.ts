/** Topic Desk Studio — minimal i18n, zero external dependencies. */

export type Locale = 'zh' | 'en'
export type Theme  = 'light' | 'dark' | 'system'

/** All translatable strings for one locale. */
export interface Messages {
  // Nav
  navDiscover: string
  navQueue: string
  navNew: string
  navSettings: string
  // Topbar
  btnRefresh: string
  btnRefreshing: string
  // Filters
  filterSource: string
  filterAllSources: string
  filterRegion: string
  filterAllRegions: string
  filterDomestic: string
  filterInternational: string
  filterCategory: string
  filterAllCategories: string
  filterGeneral: string
  filterTech: string
  filterFinance: string
  filterDev: string
  filterSearch: string
  filterSearchPlaceholder: string
  filterSort: string
  filterSortRank: string
  filterSortUpdated: string
  // Results
  resultsTotal: (n: number) => string
  resultsErrors: (n: number) => string
  resultsLoading: string
  resultsEmptyTitle: string
  resultsEmptyBody: string
  // Topic card
  rankLabel: string
  trendAccum: string
  translateBtn: string
  translating: string
  translated: string
  firstSeen: string
  consecutive: string
  consecutiveUnit: string
  queueAdd: string
  queueRemove: string
  saving: string
  // Pagination
  prev: string
  next: string
  // Settings
  settingsTitle: string
  settingsBack: string
  tabGeneral: string
  tabSources: string
  tabModel: string
  // General tab — appearance
  sectionAppearance: string
  labelTheme: string
  themeLight: string
  themeDark: string
  themeSystem: string
  // General tab — language
  sectionLanguage: string
  langZh: string
  langEn: string
  // Sources tab
  regionDomestic: string
  regionIntl: string
  notCollected: string
  topicCount: (n: number) => string
  // Model tab
  modelHeading: string
  modelDesc: string
  labelEndpoint: string
  labelModel: string
  labelApiKey: string
  apiKeyPlaceholder: string
  apiKeySavedPlaceholder: string
  btnSave: string
  btnSaving: string
  saveNotice: string
}

const zh: Messages = {
  navDiscover: '发现选题',
  navQueue: '待创作',
  navNew: '本轮新增',
  navSettings: '设置',
  btnRefresh: '刷新数据',
  btnRefreshing: '刷新中…',
  filterSource: '来源',
  filterAllSources: '全部来源',
  filterRegion: '地区',
  filterAllRegions: '全部地区',
  filterDomestic: '国内',
  filterInternational: '国外',
  filterCategory: '分类',
  filterAllCategories: '全部分类',
  filterGeneral: '综合',
  filterTech: '科技与 AI',
  filterFinance: '财经市场',
  filterDev: '开发者',
  filterSearch: '搜索',
  filterSearchPlaceholder: '输入标题关键词…',
  filterSort: '排序',
  filterSortRank: '榜单排名',
  filterSortUpdated: '最近更新',
  resultsTotal: (n) => `${n} 个选题`,
  resultsErrors: (n) => `${n} 个来源异常`,
  resultsLoading: '正在读取本地选题库…',
  resultsEmptyTitle: '本地选题库还是空的',
  resultsEmptyBody: '点击"刷新数据"开始采集，之后可以在这里筛选、追踪并加入待创作。',
  rankLabel: 'RANK',
  trendAccum: '趋势积累中',
  translateBtn: '译为中文',
  translating: '翻译中…',
  translated: '已翻译',
  firstSeen: '首次发现',
  consecutive: '连续上榜',
  consecutiveUnit: '轮',
  queueAdd: '加入待创作',
  queueRemove: '移出待创作',
  saving: '保存中…',
  prev: '上一页',
  next: '下一页',
  settingsTitle: '设置',
  settingsBack: '返回',
  tabGeneral: '通用',
  tabSources: '数据来源',
  tabModel: '翻译模型',
  sectionAppearance: '外观',
  labelTheme: '主题',
  themeLight: '浅色',
  themeDark: '深色',
  themeSystem: '随系统',
  sectionLanguage: '语言',
  langZh: '中文',
  langEn: 'English',
  regionDomestic: '国内',
  regionIntl: '国际',
  notCollected: '尚未采集',
  topicCount: (n) => `${n} 条话题`,
  modelHeading: '英文标题翻译',
  modelDesc: '接口地址与模型名保存在本地 SQLite；API Key 仅进入操作系统凭据库，不会返回页面。',
  labelEndpoint: '接口地址',
  labelModel: '模型名称',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: '输入后保存到系统凭据库',
  apiKeySavedPlaceholder: '已安全保存；留空表示不修改',
  btnSave: '保存设置',
  btnSaving: '保存中…',
  saveNotice: '模型设置已保存，API Key 已交给系统凭据库管理。',
}

const en: Messages = {
  navDiscover: 'Discover',
  navQueue: 'Queue',
  navNew: 'New this run',
  navSettings: 'Settings',
  btnRefresh: 'Refresh',
  btnRefreshing: 'Refreshing…',
  filterSource: 'Source',
  filterAllSources: 'All sources',
  filterRegion: 'Region',
  filterAllRegions: 'All regions',
  filterDomestic: 'China',
  filterInternational: 'International',
  filterCategory: 'Category',
  filterAllCategories: 'All categories',
  filterGeneral: 'General',
  filterTech: 'Tech & AI',
  filterFinance: 'Finance',
  filterDev: 'Developer',
  filterSearch: 'Search',
  filterSearchPlaceholder: 'Search by keyword…',
  filterSort: 'Sort',
  filterSortRank: 'By rank',
  filterSortUpdated: 'Latest',
  resultsTotal: (n) => `${n} topic${n !== 1 ? 's' : ''}`,
  resultsErrors: (n) => `${n} source error${n !== 1 ? 's' : ''}`,
  resultsLoading: 'Loading topics…',
  resultsEmptyTitle: 'Nothing here yet',
  resultsEmptyBody: 'Click Refresh to start collecting. Topics will appear here for you to filter, track, and queue.',
  rankLabel: 'RANK',
  trendAccum: 'Building trend',
  translateBtn: 'Translate',
  translating: 'Translating…',
  translated: 'Translated',
  firstSeen: 'First seen',
  consecutive: 'On chart',
  consecutiveUnit: 'runs',
  queueAdd: 'Add to queue',
  queueRemove: 'Remove',
  saving: 'Saving…',
  prev: 'Previous',
  next: 'Next',
  settingsTitle: 'Settings',
  settingsBack: 'Back',
  tabGeneral: 'General',
  tabSources: 'Sources',
  tabModel: 'Translation',
  sectionAppearance: 'Appearance',
  labelTheme: 'Theme',
  themeLight: 'Light',
  themeDark: 'Dark',
  themeSystem: 'System',
  sectionLanguage: 'Language',
  langZh: '中文',
  langEn: 'English',
  regionDomestic: 'China',
  regionIntl: 'Intl',
  notCollected: 'Not yet collected',
  topicCount: (n) => `${n} topic${n !== 1 ? 's' : ''}`,
  modelHeading: 'Title Translation',
  modelDesc: 'Endpoint and model name are stored locally in SQLite. Your API key goes only into the OS credential store and is never returned to the page.',
  labelEndpoint: 'Endpoint',
  labelModel: 'Model',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: 'Saved to system keychain',
  apiKeySavedPlaceholder: 'Saved securely — leave blank to keep',
  btnSave: 'Save settings',
  btnSaving: 'Saving…',
  saveNotice: 'Settings saved. API key stored in system keychain.',
}

export const messages: Record<Locale, Messages> = { zh, en }

/** Read saved locale from localStorage, falling back to browser language. */
export function detectLocale(): Locale {
  const saved = localStorage.getItem('tds-locale') as Locale | null
  if (saved === 'zh' || saved === 'en') return saved
  return navigator.language.startsWith('zh') ? 'zh' : 'en'
}

/** Read saved theme preference from localStorage. */
export function detectTheme(): Theme {
  const saved = localStorage.getItem('tds-theme') as Theme | null
  if (saved === 'light' || saved === 'dark' || saved === 'system') return saved
  return 'system'
}

/** Apply a theme choice to the document root; 'system' removes the attribute. */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement
  if (theme === 'system') {
    root.removeAttribute('data-theme')
  } else {
    root.dataset.theme = theme
  }
  localStorage.setItem('tds-theme', theme)
}
