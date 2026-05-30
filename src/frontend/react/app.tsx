import { ErrorStoreProvider } from "./lib/error-store";
import { pageComponents } from "./pages";
import type { PagePayload } from "./types";

export function App({ initialPayload }: { initialPayload: PagePayload }) {
  const Page =
    pageComponents[initialPayload.page.component] ?? pageComponents["not-found"];
  return (
    <ErrorStoreProvider>
      <Page payload={initialPayload} />
    </ErrorStoreProvider>
  );
}
