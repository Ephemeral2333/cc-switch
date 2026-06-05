import { useCallback, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { Loader2, RefreshCw, Server, TestTube2 } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { settingsApi } from "@/lib/api";
import type { ClaudeRemoteSettings as ClaudeRemoteSettingsType } from "@/types";

interface ClaudeRemoteSettingsProps {
  value?: ClaudeRemoteSettingsType;
  onChange: (value: ClaudeRemoteSettingsType) => void;
  onSyncCurrent: () => Promise<void>;
}

const DEFAULT_REMOTE: Required<
  Pick<
    ClaudeRemoteSettingsType,
    | "enabled"
    | "mode"
    | "host"
    | "port"
    | "username"
    | "remoteDir"
    | "connectTimeoutSecs"
  >
> &
  Pick<ClaudeRemoteSettingsType, "sshKeyPath"> = {
  enabled: false,
  mode: "remoteOnly",
  host: "",
  port: 22,
  username: "",
  remoteDir: "~/.claude",
  sshKeyPath: undefined,
  connectTimeoutSecs: 10,
};

export function ClaudeRemoteSettings({
  value,
  onChange,
  onSyncCurrent,
}: ClaudeRemoteSettingsProps) {
  const { t } = useTranslation();
  const [isTesting, setIsTesting] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);

  const remote = useMemo(
    () => ({
      ...DEFAULT_REMOTE,
      ...(value ?? {}),
    }),
    [value],
  );

  const updateRemote = useCallback(
    (updates: Partial<ClaudeRemoteSettingsType>) => {
      onChange({
        ...remote,
        ...updates,
      });
    },
    [onChange, remote],
  );

  const handleTest = useCallback(async () => {
    setIsTesting(true);
    try {
      await settingsApi.testClaudeRemoteConnection(remote);
      toast.success(
        t("settings.claudeRemote.testSuccess", {
          defaultValue: "远端 Claude 连接正常",
        }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("settings.claudeRemote.testFailed", {
          defaultValue: "远端 Claude 连接失败: {{error}}",
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsTesting(false);
    }
  }, [remote, t]);

  const handleSync = useCallback(async () => {
    setIsSyncing(true);
    try {
      await onSyncCurrent();
      toast.success(
        t("settings.claudeRemote.syncSuccess", {
          defaultValue: "当前 Claude 供应商已同步到远端",
        }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("settings.claudeRemote.syncFailed", {
          defaultValue: "同步远端 Claude 失败: {{error}}",
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsSyncing(false);
    }
  }, [onSyncCurrent, t]);

  return (
    <section className="space-y-5">
      <header className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-medium">
              {t("settings.claudeRemote.title", {
                defaultValue: "Claude Code 远端服务器",
              })}
            </h3>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("settings.claudeRemote.description", {
              defaultValue:
                "通过 SSH 写入远端 ~/.claude/settings.json，使用本机 SSH key 或 agent。",
            })}
          </p>
        </div>
        <Switch
          checked={remote.enabled ?? false}
          onCheckedChange={(enabled) => updateRemote({ enabled })}
        />
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <Field
          label={t("settings.claudeRemote.host", { defaultValue: "Host" })}
        >
          <Input
            value={remote.host ?? ""}
            placeholder="example.com"
            className="text-xs"
            onChange={(event) => updateRemote({ host: event.target.value })}
          />
        </Field>

        <Field
          label={t("settings.claudeRemote.port", { defaultValue: "Port" })}
        >
          <Input
            value={String(remote.port ?? 22)}
            inputMode="numeric"
            className="text-xs"
            onChange={(event) =>
              updateRemote({
                port: Number.parseInt(event.target.value, 10) || 22,
              })
            }
          />
        </Field>

        <Field
          label={t("settings.claudeRemote.username", {
            defaultValue: "Username",
          })}
        >
          <Input
            value={remote.username ?? ""}
            placeholder="ubuntu"
            className="text-xs"
            onChange={(event) =>
              updateRemote({ username: event.target.value })
            }
          />
        </Field>

        <Field
          label={t("settings.claudeRemote.mode", { defaultValue: "写入模式" })}
        >
          <Select
            value={remote.mode ?? "remoteOnly"}
            onValueChange={(mode) =>
              updateRemote({
                mode: mode as ClaudeRemoteSettingsType["mode"],
              })
            }
          >
            <SelectTrigger className="text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="remoteOnly">
                {t("settings.claudeRemote.remoteOnly", {
                  defaultValue: "仅远端",
                })}
              </SelectItem>
              <SelectItem value="localAndRemote">
                {t("settings.claudeRemote.localAndRemote", {
                  defaultValue: "本机 + 远端",
                })}
              </SelectItem>
            </SelectContent>
          </Select>
        </Field>

        <Field
          label={t("settings.claudeRemote.remoteDir", {
            defaultValue: "远端 Claude 目录",
          })}
        >
          <Input
            value={remote.remoteDir ?? "~/.claude"}
            placeholder="~/.claude"
            className="text-xs"
            onChange={(event) =>
              updateRemote({ remoteDir: event.target.value })
            }
          />
        </Field>

        <Field
          label={t("settings.claudeRemote.timeout", {
            defaultValue: "连接超时（秒）",
          })}
        >
          <Input
            value={String(remote.connectTimeoutSecs ?? 10)}
            inputMode="numeric"
            className="text-xs"
            onChange={(event) =>
              updateRemote({
                connectTimeoutSecs:
                  Number.parseInt(event.target.value, 10) || 10,
              })
            }
          />
        </Field>

        <div className="md:col-span-2">
          <Field
            label={t("settings.claudeRemote.sshKeyPath", {
              defaultValue: "SSH key path",
            })}
          >
            <Input
              value={remote.sshKeyPath ?? ""}
              placeholder="~/.ssh/id_ed25519"
              className="text-xs"
              onChange={(event) =>
                updateRemote({ sshKeyPath: event.target.value || undefined })
              }
            />
          </Field>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={isTesting}
          onClick={handleTest}
        >
          {isTesting ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <TestTube2 className="mr-2 h-4 w-4" />
          )}
          {t("settings.claudeRemote.test", { defaultValue: "测试连接" })}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={isSyncing || !remote.enabled}
          onClick={handleSync}
        >
          {isSyncing ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <RefreshCw className="mr-2 h-4 w-4" />
          )}
          {t("settings.claudeRemote.syncCurrent", {
            defaultValue: "同步当前供应商",
          })}
        </Button>
      </div>
    </section>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="space-y-1.5 block">
      <span className="text-xs font-medium text-foreground">{label}</span>
      {children}
    </label>
  );
}
