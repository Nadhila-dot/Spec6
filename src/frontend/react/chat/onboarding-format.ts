/**
 * Structured company-context format. The user fills in discrete fields in the
 * onboarding form; we serialize them into the `data_text` string that the
 * backend stores on the ChatGroup.
 *
 * IMPORTANT: the serialization format must match `parse_company_seed` /
 * `extract_labeled_value` in src/overview.rs — that backend function reads
 * `- Label: value` lines (single line each) and uses them to seed the
 * competitor-research pipeline. If we don't match these labels, the overview
 * silently never queues because every field comes back empty.
 *
 * Labels (must match backend exactly):
 *   - Company name
 *   - Website
 *   - Specializes in
 *   - Customers or target markets
 *   - Known competitors
 *   - Additional notes
 */

export interface CompanyOnboarding {
  url: string;
  specialty: string;
  customers: string;
  competitors: string;
  notes: string;
}

export const EMPTY_ONBOARDING: CompanyOnboarding = {
  url: "",
  specialty: "",
  customers: "",
  competitors: "",
  notes: "",
};

const FIELDS: Array<{ key: keyof CompanyOnboarding; label: string }> = [
  { key: "url",         label: "Website" },
  { key: "specialty",   label: "Specializes in" },
  { key: "customers",   label: "Customers or target markets" },
  { key: "competitors", label: "Known competitors" },
  { key: "notes",       label: "Additional notes" },
];

/** Multi-line input is collapsed to a single line for backend parsing. */
function singleLine(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

export function serializeOnboarding(onboarding: CompanyOnboarding): string {
  const lines: string[] = [];
  for (const { key, label } of FIELDS) {
    const value = singleLine(onboarding[key]);
    if (value) lines.push(`- ${label}: ${value}`);
  }
  return lines.join("\n");
}

export function parseOnboarding(dataText: string): CompanyOnboarding {
  const result: CompanyOnboarding = { ...EMPTY_ONBOARDING };
  if (!dataText.trim()) return result;
  for (const rawLine of dataText.split("\n")) {
    const line = rawLine.trim();
    if (!line.startsWith("- ")) continue;
    for (const { key, label } of FIELDS) {
      const prefix = `- ${label}:`;
      if (line.startsWith(prefix)) {
        result[key] = line.slice(prefix.length).trim();
        break;
      }
    }
  }
  return result;
}

export function isOnboardingEmpty(o: CompanyOnboarding): boolean {
  return (
    !o.url.trim() &&
    !o.specialty.trim() &&
    !o.customers.trim() &&
    !o.competitors.trim() &&
    !o.notes.trim()
  );
}

/* ─── demo content (Puma) ───────────────────────────────────────────────── */

export const DEMO_COMPANY_NAME = "Puma";

export const DEMO_ONBOARDING: CompanyOnboarding = {
  url: "https://us.puma.com/",
  specialty:
    "Athletic footwear, football kits, motorsport apparel, and fashion-forward sportswear.",
  customers:
    "Style-aware sneaker buyers, football fans, younger streetwear consumers, and shoppers who want value below Nike and Adidas.",
  competitors: "Nike, Adidas, New Balance, Under Armour, Asics",
  notes:
    "Pressure-test customer trust, return experience, product quality sentiment, and where Puma can win on value, design taste, and football credibility.",
};
