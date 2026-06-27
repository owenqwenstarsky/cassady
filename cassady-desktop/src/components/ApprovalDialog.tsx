import { type PendingApproval } from "@/hooks/useTurn";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";

export function ApprovalDialog({
  approval,
  onResolve,
}: {
  approval: PendingApproval;
  onResolve: (approved: boolean) => void;
}) {
  const args = approval.arguments
    ? JSON.stringify(approval.arguments, null, 2)
    : "";
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-lg">
        <CardHeader>
          <CardTitle className="text-[var(--color-amber)]">
            approval required
          </CardTitle>
          <CardDescription>
            <span className="font-mono text-[var(--color-fg)]">
              {approval.name}
            </span>{" "}
            — {approval.reason}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {args && (
            <pre className="max-h-60 overflow-auto border border-[var(--color-line)] bg-[var(--color-bg)] p-3 font-mono text-xs text-[var(--color-fg-muted)] whitespace-pre-wrap break-words">
              {args}
            </pre>
          )}
        </CardContent>
        <CardFooter className="gap-3 justify-end">
          <Button variant="outline" onClick={() => onResolve(false)}>
            deny
          </Button>
          <Button onClick={() => onResolve(true)}>approve</Button>
        </CardFooter>
      </Card>
    </div>
  );
}
