# UI brief: HooksSetupDialog

Scope: one small modal copying existing components. No new design; no new tokens.

- Product type / style: desktop dev tool, dark-first, existing Nergal design system (docs/design.md). No new palette, fonts or motion.
- Components: Dialog/DialogContent/Header/Title/Description/Footer, Button (secondary + default), Kbd (esc, enter tone onPrimary). Layout copied from ProjectBootstrapPrompt.
- Tokens: text-muted-foreground for body, text-foreground for emphasis, text-destructive for the inline error, warning line uses existing muted/warning text, no raw colors.
- Keyboard (ConfirmHost model): showCloseButton={false}, tabIndex={-1}, rAF focus on own DialogContent via ref, onKeyDownCapture always consumes Enter (e.key), Configure only when canConfigure && !busy; Esc via onOpenChange -> Skip, blocked while busy.
- Focus: no button ring (focus-visible:ring-0 focus-visible:border-transparent, BUG-25); Kbd chips are the affordance. No new shortcuts bound (dialog-local keys only).
- UX laws: one primary action (Configure); feedback via busy label + toast; error inline and recoverable (stay open); disabled Configure explains why (nergal not on PATH); minimal copy.
- Copy: English. Anti-slop: no emojis, no decorative gradients.
