import { Button } from "../components/button";
import { Card } from "../components/card";
import { HatchedChip } from "../components/diagonal";
import { MetaFooter } from "../components/footer";
import { IconArrowRight, IconSparkle } from "../components/icons";
import type { NotFoundProps, PagePayload } from "../types";

export function NotFoundPage({ payload }: { payload: PagePayload }) {
  const props = payload.page.props as unknown as NotFoundProps;

  return (
    <div className="flex min-h-full flex-col px-4 py-6 sm:px-8 sm:py-10">
      <div className="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center">
        <Card innerClassName="gap-5 p-7">
          <div className="flex items-center gap-2.5">
            <HatchedChip size={32}>
              <IconSparkle size={15} />
            </HatchedChip>
            <span className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-rose-400">
              404 · NOT FOUND
            </span>
          </div>
          <h1 className="font-chillax text-[28px] font-semibold leading-tight tracking-tight text-foreground">
            No route at this address.
          </h1>
          <p className="text-[13px] leading-[1.55] text-muted-foreground/80">
            {props.message}
          </p>
          <p className="font-mono text-[11px] tabular-nums text-muted-foreground/55">
            {props.path}
          </p>
          <a href="/">
            <Button size="lg">
              Back home
              <IconArrowRight size={15} />
            </Button>
          </a>
        </Card>
      </div>
      <MetaFooter />
    </div>
  );
}
