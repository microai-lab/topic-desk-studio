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
  btnQuery: string
  btnQuerying: string
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
  tabStorage: string
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
  sectionNetwork: string
  labelProxy: string
  proxyPlaceholder: string
  proxyHelp: string
  networkSaveNotice: string
  storageHeading: string
  storageDesc: string
  storageHealthy: string
  storageDamaged: string
  storageTopics: string
  storageTrends: string
  storageRuns: string
  storageBrowser: string
  storageTopicDb: string
  storageBrowserDb: string
  storageLatestBackup: string
  storageNoBackup: string
  storageOpenFolder: string
  storageOptimize: string
  storageBackup: string
  storageRestore: string
  storageWorking: string
  storageRestoreConfirm: string
  // Sources tab
  regionDomestic: string
  regionIntl: string
  notCollected: string
  topicCount: (n: number) => string
  xhsLoginCollect: string
  xhsCollectPage: string
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
  btnRefresh: '采集数据',
  btnRefreshing: '采集中…',
  btnQuery: '刷新列表',
  btnQuerying: '查询中…',
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
  resultsEmptyBody: '点击“采集数据”开始采集，之后可以在这里筛选、追踪并加入待创作。',
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
  tabStorage: '本地数据',
  sectionAppearance: '外观',
  labelTheme: '主题',
  themeLight: '浅色',
  themeDark: '深色',
  themeSystem: '随系统',
  sectionLanguage: '语言',
  langZh: '中文',
  langEn: 'English',
  sectionNetwork: '网络代理',
  labelProxy: 'HTTP(S) 代理',
  proxyPlaceholder: '例如 http://127.0.0.1:7897',
  proxyHelp: '仅直连受限的境外来源使用该代理；国内及可直连来源保持直连。留空表示全部直连。',
  networkSaveNotice: '采集代理设置已保存。',
  storageHeading: '本地数据管理',
  storageDesc: '话题、采集历史和浏览记录均保存在本机。系统会自动汇总趋势并清理过期运行记录。',
  storageHealthy: '数据库状态正常',
  storageDamaged: '数据库完整性异常',
  storageTopics: '话题',
  storageTrends: '趋势记录',
  storageRuns: '采集运行',
  storageBrowser: '浏览记录',
  storageTopicDb: '选题数据库',
  storageBrowserDb: '浏览数据库',
  storageLatestBackup: '最近备份',
  storageNoBackup: '尚未创建',
  storageOpenFolder: '打开数据目录',
  storageOptimize: '立即整理',
  storageBackup: '创建备份',
  storageRestore: '恢复最近备份',
  storageWorking: '处理中…',
  storageRestoreConfirm: '恢复将覆盖当前本地数据，并保留下载文件。确定继续吗？',
  regionDomestic: '国内',
  regionIntl: '国际',
  notCollected: '尚未采集',
  topicCount: (n) => `${n} 条话题`,
  xhsLoginCollect: '登录采集',
  xhsCollectPage: '采集当前页',
  modelHeading: '英文标题翻译',
  modelDesc: '接口地址与模型名保存在本地 SQLite；API Key 加密后入库，仅在调用模型时于内存解密。',
  labelEndpoint: '接口地址',
  labelModel: '模型名称',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: '输入后加密保存到本地 SQLite',
  apiKeySavedPlaceholder: '已保存；留空表示不修改',
  btnSave: '保存设置',
  btnSaving: '保存中…',
  saveNotice: '模型设置已保存到本地 SQLite。',
}

const en: Messages = {
  navDiscover: 'Discover',
  navQueue: 'Queue',
  navNew: 'New this run',
  navSettings: 'Settings',
  btnRefresh: 'Collect',
  btnRefreshing: 'Collecting…',
  btnQuery: 'Refresh list',
  btnQuerying: 'Querying…',
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
  resultsEmptyBody: 'Click Collect to start collecting. Topics will appear here for you to filter, track, and queue.',
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
  tabStorage: 'Local data',
  sectionAppearance: 'Appearance',
  labelTheme: 'Theme',
  themeLight: 'Light',
  themeDark: 'Dark',
  themeSystem: 'System',
  sectionLanguage: 'Language',
  langZh: '中文',
  langEn: 'English',
  sectionNetwork: 'Network proxy',
  labelProxy: 'HTTP(S) proxy',
  proxyPlaceholder: 'For example http://127.0.0.1:7897',
  proxyHelp: 'Only restricted international sources use this proxy. Domestic and directly reachable sources remain direct. Leave blank to make every source direct.',
  networkSaveNotice: 'Collection proxy saved.',
  storageHeading: 'Local Data',
  storageDesc: 'Topics, collection history, and browser records stay on this device. Trends are rolled up and expired run data is pruned automatically.',
  storageHealthy: 'Databases healthy',
  storageDamaged: 'Database integrity issue',
  storageTopics: 'Topics',
  storageTrends: 'Trend records',
  storageRuns: 'Collection runs',
  storageBrowser: 'Browser records',
  storageTopicDb: 'Topic database',
  storageBrowserDb: 'Browser database',
  storageLatestBackup: 'Latest backup',
  storageNoBackup: 'Not created yet',
  storageOpenFolder: 'Open data folder',
  storageOptimize: 'Optimize now',
  storageBackup: 'Create backup',
  storageRestore: 'Restore latest',
  storageWorking: 'Working…',
  storageRestoreConfirm: 'Restoring replaces current local data but keeps downloaded files. Continue?',
  regionDomestic: 'China',
  regionIntl: 'Intl',
  notCollected: 'Not yet collected',
  topicCount: (n) => `${n} topic${n !== 1 ? 's' : ''}`,
  xhsLoginCollect: 'Login & collect',
  xhsCollectPage: 'Collect page',
  modelHeading: 'Title Translation',
  modelDesc: 'Endpoint and model name are stored in SQLite. The API key is encrypted at rest and decrypted in memory only for model calls.',
  labelEndpoint: 'Endpoint',
  labelModel: 'Model',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: 'Encrypted in local SQLite',
  apiKeySavedPlaceholder: 'Saved — leave blank to keep',
  btnSave: 'Save settings',
  btnSaving: 'Saving…',
  saveNotice: 'Model settings saved to local SQLite.',
}

export const messages: Record<Locale, Messages> = { zh, en }

/** Choose the initial locale before Rust-backed preferences finish loading. */
export function detectLocale(): Locale {
  return navigator.language.startsWith('zh') ? 'zh' : 'en'
}

/** Use the operating-system appearance until Rust-backed preferences finish loading. */
export function detectTheme(): Theme {
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
}
