// Core types for browser interaction

export interface SemanticElement {
  id?: string;
  role: string;
  name?: string;
  text?: string;
  href?: string;
  inputType?: string;
  placeholder?: string;
  children?: SemanticElement[];
}

export interface SemanticTree {
  url: string;
  title?: string;
  markdown: string;
  stats: {
    landmarks: number;
    links: number;
    headings: number;
    actions: number;
    forms: number;
    totalNodes: number;
  };
}

export interface BrowserNavigateResult {
  success: boolean;
  title?: string;
  url: string;
  markdown: string;
  stats: SemanticTree['stats'];
  error?: string;
}

export interface BrowserClickResult {
  success: boolean;
  navigated: boolean;
  url?: string;
  markdown?: string;
  stats?: SemanticTree['stats'];
  error?: string;
}

export interface BrowserFillResult {
  success: boolean;
  error?: string;
}

export interface BrowserSubmitResult {
  success: boolean;
  navigated: boolean;
  url?: string;
  markdown?: string;
  stats?: SemanticTree['stats'];
  error?: string;
}

export interface BrowserScrollResult {
  success: boolean;
  /** Updated semantic tree after scrolling */
  markdown?: string;
  error?: string;
}

// Cookie types
export interface Cookie {
  name: string;
  value: string;
  domain?: string;
  path?: string;
  expires?: number;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: 'Strict' | 'Lax' | 'None';
}

export interface BrowserGetCookiesResult {
  success: boolean;
  cookies: Cookie[];
  error?: string;
}

export interface BrowserSetCookieResult {
  success: boolean;
  error?: string;
}

export interface BrowserDeleteCookieResult {
  success: boolean;
  error?: string;
}

// Storage types
export interface StorageItem {
  key: string;
  value: string;
}

export interface BrowserGetStorageResult {
  success: boolean;
  items: StorageItem[];
  error?: string;
}

export interface BrowserSetStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserDeleteStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserClearStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserInstanceInfo {
  id: string;
  url?: string;
  connected: boolean;
  port: number;
}

export interface ToolResult {
  success: boolean;
  content: string;
  error?: string;
}

export interface SuggestedAction {
  action_type: string;
  element_id?: number;
  selector?: string;
  label?: string;
  reason: string;
  confidence: number;
}

export interface ActionPlanResult {
  url: string;
  suggestions: SuggestedAction[];
  page_type: string;
  has_forms: boolean;
  has_pagination: boolean;
  interactive_count: number;
}

export interface BrowserGetActionPlanResult {
  success: boolean;
  actionPlan?: ActionPlanResult;
  error?: string;
}

export interface FilledField {
  field_name: string;
  value: string;
  matched_by: string;
}

export interface UnmatchedField {
  field_name?: string;
  field_type: string;
  label?: string;
  placeholder?: string;
  required: boolean;
}

export interface BrowserAutoFillResult {
  success: boolean;
  filledFields?: FilledField[];
  unmatchedFields?: UnmatchedField[];
  error?: string;
}

export interface BrowserWaitResult {
  success: boolean;
  satisfied: boolean;
  condition: string;
  reason?: string;
  error?: string;
}

// ── Extraction types ──────────────────────────────────────────────

export interface BrowserExtractTextResult {
  success: boolean;
  text: string;
  word_count: number;
  error?: string;
}

export interface LinkItem {
  text: string;
  href: string;
  element_id?: string;
}

export interface BrowserExtractLinksResult {
  success: boolean;
  links: LinkItem[];
  count: number;
  error?: string;
}

export interface TextMatch {
  text: string;
  context: string;
  element_id?: string;
}

export interface BrowserFindResult {
  success: boolean;
  matches: TextMatch[];
  count: number;
  error?: string;
}

export interface BrowserExtractTableResult {
  success: boolean;
  headers: string[];
  rows: string[][];
  row_count: number;
  error?: string;
}

export interface BrowserExtractMetadataResult {
  success: boolean;
  title: string;
  description?: string;
  json_ld: unknown[];
  open_graph: Record<string, string>;
  meta: Record<string, string>;
  error?: string;
}

export interface BrowserScreenshotResult {
  success: boolean;
  data: string; // base64-encoded image
  mime_type: string;
  error?: string;
}

// ── Interaction types ─────────────────────────────────────────────

export interface BrowserSelectResult {
  success: boolean;
  selected_value: string;
  error?: string;
}

export interface BrowserPressKeyResult {
  success: boolean;
  error?: string;
}

export interface BrowserHoverResult {
  success: boolean;
  error?: string;
}

// ── Tab management types ──────────────────────────────────────────

export interface TabInfo {
  target_id: string;
  url: string;
  title: string;
  active: boolean;
}

export interface BrowserTabNewResult {
  success: boolean;
  target_id: string;
  error?: string;
}

export interface BrowserTabSwitchResult {
  success: boolean;
  error?: string;
}

export interface BrowserTabCloseResult {
  success: boolean;
  error?: string;
}

export interface BrowserTabListResult {
  success: boolean;
  tabs: TabInfo[];
  error?: string;
}

// ── Download/Upload types ──────────────────────────────────────────

export interface BrowserDownloadResult {
  success: boolean;
  path: string;
  size_bytes: number;
  mime_type?: string;
  error?: string;
}

export interface BrowserUploadResult {
  success: boolean;
  error?: string;
}

// ── Content types ──────────────────────────────────────────────────

export interface BrowserPdfExtractResult {
  success: boolean;
  text: string;
  page_count: number;
  tables?: string[][];
  forms?: Record<string, string>[];
  error?: string;
}

export interface FeedItem {
  title: string;
  link: string;
  description?: string;
  pub_date?: string;
  author?: string;
  categories?: string[];
}

export interface BrowserFeedParseResult {
  success: boolean;
  feed_type: 'rss' | 'atom';
  title: string;
  description?: string;
  items: FeedItem[];
  item_count: number;
  error?: string;
}

// ── Network control types ──────────────────────────────────────────

export interface BrowserNetworkBlockResult {
  success: boolean;
  blocked_types: string[];
  error?: string;
}

export interface BrowserNetworkLogResult {
  success: boolean;
  requests: Array<{
    url: string;
    method: string;
    status: number;
    mime_type: string;
    size_bytes: number;
    duration_ms: number;
  }>;
  count: number;
  error?: string;
}

// ── Iframe types ───────────────────────────────────────────────────

export interface BrowserIframeEnterResult {
  success: boolean;
  error?: string;
}

export interface BrowserIframeExitResult {
  success: boolean;
  error?: string;
}

// ── Page diff types ────────────────────────────────────────────────

export interface PageDiffChange {
  type: 'added' | 'removed' | 'modified';
  selector: string;
  text?: string;
  old_text?: string;
  new_text?: string;
}

export interface BrowserDiffResult {
  success: boolean;
  changes: PageDiffChange[];
  change_count: number;
  summary: string;
  error?: string;
}
