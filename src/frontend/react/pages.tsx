import type { ComponentType } from "react";
import { ChatPage } from "./pages/chat-page";
import { LandingPage } from "./pages/landing-page";
import { LoginPage } from "./pages/login-page";
import { NotFoundPage } from "./pages/not-found-page";
import { SignupPage } from "./pages/signup-page";
import type { PageComponent, PagePayload } from "./types";

export const pageComponents: Record<
  PageComponent,
  ComponentType<{ payload: PagePayload }>
> = {
  landing: LandingPage,
  login: () => <LoginPage />,
  signup: () => <SignupPage />,
  chat: ChatPage,
  "not-found": NotFoundPage,
};
