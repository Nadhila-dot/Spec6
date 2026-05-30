import { Button } from "../components/button";
import { Card } from "../components/card";
import { DiagonalAccent } from "../components/diagonal";
import { MetaFooter } from "../components/footer";
import { IconArrowRight } from "../components/icons";
import type { LandingProps, PagePayload } from "../types";

export function LandingPage({ payload }: { payload: PagePayload }) {
  const props = payload.page.props as unknown as LandingProps;
  const user = payload.user;

  return (
    <div className="flex min-h-full flex-col px-4 py-6 sm:px-8 sm:py-10">
      <div className="mx-auto flex w-full max-w-3xl flex-1 flex-col justify-center gap-8">
        <BrandRow />

        <div className="relative overflow-hidden rounded-2xl bg-card shadow-[0_6px_28px_rgba(0,0,0,0.24)]">
          <div className="absolute inset-0 bg-gradient-to-br from-zinc-700 via-zinc-900 to-zinc-950" />
          <div className="diagonal-line-corner absolute inset-0" />
          <div className="absolute inset-0 bg-gradient-to-r from-background/30 via-background/5 to-transparent" />

          <div className="relative z-10 flex flex-col gap-5 px-6 py-8 sm:px-10 sm:py-12">
            <h1
              className="font-chillax text-[34px] font-semibold leading-[1.05] tracking-tight text-white sm:text-[44px]"
              style={{ textShadow: "0 4px 12px rgba(0,0,0,0.72)" }}
            >
              {props.tagline}
            </h1>

            <p className="max-w-xl text-[13.5px] leading-[1.55] text-white/75">
              {props.copy}
            </p>

            <div className="flex flex-wrap items-center gap-2">
              {user ? (
                <a href="/chat">
                  <Button size="lg">
                    Open Sentinel
                    <IconArrowRight size={15} />
                  </Button>
                </a>
              ) : (
                <>
                  <a href="/signup">
                    <Button size="lg">
                      Create an account
                      <IconArrowRight size={15} />
                    </Button>
                  </a>
                  <a href="/login">
                    <Button size="lg" variant="outline" className="text-white ring-white/15">
                      Sign in
                    </Button>
                  </a>
                </>
              )}
            </div>
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-3">
          <FeatureCard
            title="Eight agents, one brief."
            body="Counterfeit, competitive, reputation, and supply-chain signals fan out concurrently and synthesize into a single dossier."
          />
          <FeatureCard
            title="90-second turnaround."
            body="One brand name in, an analyst-grade threat report out — replacing a four-tool, six-figure workflow."
          />
          <FeatureCard
            title="Built on BrightData."
            body="Web Unlocker, SERP API, Scraping Browser, Web Scraper API, Scraper Studio, Proxies, and MCP — the full suite."
          />
        </div>
      </div>

      <MetaFooter />
    </div>
  );
}

function BrandRow() {
  return (
    <div className="flex items-center">
      <span className="font-chillax text-[17px] font-semibold tracking-tight text-foreground/95">
        Sentinel
      </span>
      <span className="ml-auto text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/60">
        v0.1.0
      </span>
    </div>
  );
}

function FeatureCard({
  title,
  body,
}: {
  title: string;
  body: string;
}) {
  return (
    <Card innerClassName="gap-2 p-5">
      <h3 className="font-chillax text-[15px] font-semibold tracking-tight text-foreground">
        {title}
      </h3>
      <p className="relative text-[12.5px] leading-[1.55] text-muted-foreground/85">
        <DiagonalAccent
          className="text-foreground -m-1 rounded-md"
          opacity={0.018}
          spacing={7}
        />
        <span className="relative">{body}</span>
      </p>
    </Card>
  );
}
