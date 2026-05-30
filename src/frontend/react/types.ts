export type PageComponent =
  | "landing"
  | "login"
  | "signup"
  | "chat"
  | "not-found";

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
}

export interface PagePayload {
  request: {
    path: string;
    status: number;
  };
  meta: {
    title: string;
    description: string;
    locale: string;
    url: string;
  };
  page: {
    component: PageComponent;
    props: Record<string, unknown>;
  };
  user?: AuthUser;
}

export interface LandingProps {
  tagline: string;
  copy: string;
}

export interface NotFoundProps {
  path: string;
  message: string;
}

export interface ChatProps {
  conversation_id: string | null;
}

export interface InferenceModelSummary {
  id: string;
  label: string;
}

export interface InferenceCatalogProvider {
  id: string;
  label: string;
  available: boolean;
  default_model: string | null;
  models: InferenceModelSummary[];
  error: string | null;
}

export interface InferenceCatalog {
  default_provider: string | null;
  providers: InferenceCatalogProvider[];
}

export interface Conversation {
  id: string;
  group_id: string | null;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface ChatGroup {
  id: string;
  name: string;
  data_text: string;
  created_at: string;
  updated_at: string;
}

export type CompanyOverviewStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed";

export interface CompanyOverviewSummary {
  actual_competitors: string;
  customer_trust_and_desire_to_use: string;
  faults: string;
  rating: string;
  where_to_do_better: string;
  how_long_this_will_last: string;
  market_saturation_and_overlap: string;
  confidence_notes: string;
}

export interface CompanyOverviewEvidence {
  source_type: string;
  label: string;
  url?: string | null;
  snippet: string;
  rating?: number | null;
  review_count?: number | null;
}

export interface CompanyOverviewCompetitor {
  name: string;
  domain?: string | null;
  website_url?: string | null;
  classification: string;
  score: number;
  overlap_summary: string;
  customer_trust: string;
  faults: string;
  rating_summary: string;
  where_to_do_better: string;
  durability_summary: string;
  saturation_summary: string;
  evidence: CompanyOverviewEvidence[];
}

export interface CompanyOverview {
  company_id: string;
  company_name: string;
  status: CompanyOverviewStatus;
  started_at?: string | null;
  completed_at?: string | null;
  discovered_competitors: CompanyOverviewCompetitor[];
  summary?: CompanyOverviewSummary | null;
  markdown_brief: string;
  failure_reason?: string | null;
  created_at: string;
  updated_at: string;
}

export type MessageRole = "user" | "assistant";

export interface ChatMessage {
  id: string;
  role: MessageRole;
  body: string;
  created_at: string;
}
