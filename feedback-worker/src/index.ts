/**
 * CoH Planner Feedback Worker
 *
 * Receives feedback form submissions and sends formatted emails via Resend.
 * Deploy: `wrangler deploy`
 * Secrets: `wrangler secret put RESEND_API_KEY` and `wrangler secret put FEEDBACK_EMAIL`,
 * plus `wrangler secret put DESKTOP_CLIENT_TOKEN` for the desktop arm below.
 */

interface Env {
  RESEND_API_KEY: string;
  FEEDBACK_EMAIL: string;
  /**
   * Shared secret the desktop build sends as `X-Sidekick-Desktop`. Unset means the desktop arm
   * is closed and every native submission 403s, which is the behaviour this worker had before
   * the header existed — see `isDesktopClient`.
   */
  DESKTOP_CLIENT_TOKEN?: string;
}

interface BuildContext {
  archetype: string;
  level: number;
  primary: string;
  secondary: string;
  pools: string[];
  epicPool: string | null;
  powerCount: number;
  slotCount: number;
}

interface DiagnosticsSnapshot {
  app?: { version?: string; buildTime?: string };
  env?: {
    userAgent?: string;
    viewport?: { width?: number; height?: number };
    datasetId?: string;
    url?: string;
  };
  ui?: Record<string, unknown>;
}

interface FeedbackPayload {
  type: 'bug' | 'suggestion' | 'other';
  description: string;
  globalName?: string;
  /** Supabase account UUID, auto-attached when the submitter is logged in */
  userId?: string;
  /** Display name of the signed-in account (Discord name / email) */
  userName?: string;
  buildContext?: BuildContext;
  buildSnapshot?: string;
  diagnostics?: DiagnosticsSnapshot;
  userAgent: string;
  timestamp: string;
}

const ALLOWED_ORIGINS = [
  'https://coh-sidekick.com',
  'https://wednesdaywoe.github.io',
  'http://localhost:3000',
];

/** The header the desktop build carries in place of an `Origin` the browser would have set. */
const DESKTOP_CLIENT_HEADER = 'X-Sidekick-Desktop';

/**
 * Is this the desktop build?
 *
 * The origin allow-list above is browser hygiene: it stops another site's page posting here with
 * a user's cookies attached. It was never a defence against a native client, which sets whatever
 * headers it likes — and native `reqwest` sets no `Origin` at all, so the desktop app 403'd on
 * every submission from the day it shipped (F35). This is the door for it.
 *
 * It is a shared secret rather than a marker header because a marker anyone can read in a public
 * repository admits anyone, and this worker spends Resend mail on what it admits. A token baked
 * into a shipped binary is not a real secret either — it raises the cost from reading a source
 * file to unpacking a bundle, and no further. What bounds the damage is that the token is
 * rotatable here without shipping a new build to anyone but the RC group.
 *
 * Fails closed: an unconfigured `DESKTOP_CLIENT_TOKEN` refuses every native submission rather
 * than admitting all of them.
 */
function isDesktopClient(request: Request, env: Env): boolean {
  const expected = env.DESKTOP_CLIENT_TOKEN;
  if (!expected) return false;
  const presented = request.headers.get(DESKTOP_CLIENT_HEADER);
  if (!presented) return false;
  return timingSafeEqual(presented, expected);
}

/** Compare without leaking the matching prefix through timing. */
function timingSafeEqual(a: string, b: string): boolean {
  const encoder = new TextEncoder();
  const left = encoder.encode(a);
  const right = encoder.encode(b);
  if (left.length !== right.length) return false;
  let diff = 0;
  for (let i = 0; i < left.length; i += 1) diff |= left[i] ^ right[i];
  return diff === 0;
}

function getCorsHeaders(origin: string | null): Record<string, string> {
  const allowed = origin && ALLOWED_ORIGINS.some(o => origin.startsWith(o));
  return {
    'Access-Control-Allow-Origin': allowed ? origin! : ALLOWED_ORIGINS[0],
    'Access-Control-Allow-Methods': 'POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  };
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function fmtVal(v: unknown): string {
  if (v === null || v === undefined) return '<em style="color:#64748b">—</em>';
  if (typeof v === 'boolean') return v
    ? '<span style="color:#34d399">on</span>'
    : '<span style="color:#94a3b8">off</span>';
  if (typeof v === 'number' || typeof v === 'string') return escapeHtml(String(v));
  if (Array.isArray(v)) {
    return v.length === 0
      ? '<em style="color:#64748b">(empty)</em>'
      : escapeHtml(v.map(String).join(', '));
  }
  // object — show keys with truthy values, or full JSON if small
  const json = JSON.stringify(v);
  if (json.length <= 80) return `<code style="color:#e2e8f0">${escapeHtml(json)}</code>`;
  return `<code style="color:#e2e8f0">${escapeHtml(json.slice(0, 80))}…</code>`;
}

function diagnosticsRow(label: string, value: unknown): string {
  return `<tr>
    <td style="color:#64748b; padding:3px 8px; font-size:12px;">${escapeHtml(label)}</td>
    <td style="color:#e2e8f0; padding:3px 8px; font-size:12px;">${fmtVal(value)}</td>
  </tr>`;
}

function buildDiagnosticsHtml(d: { app?: any; env?: any; ui?: any }): string {
  const app = d.app ?? {};
  const env = d.env ?? {};
  const ui = d.ui ?? {};
  const viewport = env.viewport ? `${env.viewport.width}×${env.viewport.height}` : null;

  const rows = [
    diagnosticsRow('App version', app.version),
    diagnosticsRow('Build time', app.buildTime),
    diagnosticsRow('Dataset', env.datasetId),
    diagnosticsRow('Viewport', viewport),
    diagnosticsRow('URL', env.url),
    diagnosticsRow('Level Up Mode', ui.levelUpMode),
    diagnosticsRow('Combat Mode', ui.combatMode),
    diagnosticsRow('Exemplar Mode', ui.exemplarMode === true ? `on (lvl ${ui.exemplarLevel})` : false),
    diagnosticsRow('Attunement', ui.attunementEnabled),
    diagnosticsRow('Global IO Level', ui.globalIOLevel),
    diagnosticsRow('Global Boost', ui.globalBoostLevel),
    diagnosticsRow('Target Level Offset', ui.targetLevelOffset),
    diagnosticsRow('ArcanaTime', ui.useArcanaTime),
    diagnosticsRow('Procs in DPS', ui.includeProcDamageInDPS),
    diagnosticsRow('Proc Settings', ui.procSettings),
    diagnosticsRow('Damage Display', ui.damageDisplayMode),
    diagnosticsRow('Power View', ui.powerViewMode),
    diagnosticsRow('Selected Branch', ui.selectedBranch),
    diagnosticsRow('Incarnate Level Shift', ui.incarnateLevelShiftActive),
    diagnosticsRow('Incarnate Active', ui.incarnateActive),
    diagnosticsRow('Mechanic Adjusters (on)', ui.mechanicAdjusters),
    diagnosticsRow('Global Adjusters (on)', ui.globalAdjusters),
    diagnosticsRow('Targets Hit (non-zero)', ui.targetsHitValues),
    diagnosticsRow('Tracked Stats', ui.trackedStats),
    diagnosticsRow('Perma Tracked', ui.permaTrackedPowers),
  ];

  // AT-conditional keys — render any UI key not already shown above
  const renderedKeys = new Set([
    'levelUpMode', 'combatMode', 'exemplarMode', 'exemplarLevel',
    'attunementEnabled', 'globalIOLevel', 'globalBoostLevel',
    'targetLevelOffset', 'procSettings', 'includeProcDamageInDPS',
    'useArcanaTime', 'damageDisplayMode', 'incarnateActive',
    'incarnateLevelShiftActive', 'selectedBranch', 'powerViewMode',
    'targetsHitValues', 'mechanicAdjusters', 'globalAdjusters',
    'trackedStats', 'permaTrackedPowers',
  ]);
  for (const [key, value] of Object.entries(ui)) {
    if (renderedKeys.has(key)) continue;
    rows.push(diagnosticsRow(key, value));
  }

  return `
    <h3 style="color: #94a3b8; margin-top: 16px;">Diagnostics</h3>
    <table style="width: 100%; border-collapse: collapse;">
      ${rows.join('')}
    </table>`;
}

function buildEmailHtml(payload: FeedbackPayload): string {
  const typeLabel: Record<string, string> = {
    bug: 'Bug Report',
    suggestion: 'Feature Suggestion',
    other: 'General Feedback',
  };

  const typeColor: Record<string, string> = {
    bug: '#ef4444',
    suggestion: '#3b82f6',
    other: '#8b5cf6',
  };

  const label = typeLabel[payload.type] || 'Feedback';
  const color = typeColor[payload.type] || '#8b5cf6';

  let contextHtml = '';
  if (payload.buildContext) {
    const ctx = payload.buildContext;
    contextHtml = `
      <h3 style="color: #94a3b8; margin-top: 16px;">Build Context</h3>
      <table style="width: 100%; border-collapse: collapse;">
        <tr><td style="color: #64748b; padding: 4px 8px;">Archetype</td><td style="color: #e2e8f0; padding: 4px 8px;">${escapeHtml(ctx.archetype)}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Level</td><td style="color: #e2e8f0; padding: 4px 8px;">${ctx.level}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Primary</td><td style="color: #e2e8f0; padding: 4px 8px;">${escapeHtml(ctx.primary)}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Secondary</td><td style="color: #e2e8f0; padding: 4px 8px;">${escapeHtml(ctx.secondary)}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Pools</td><td style="color: #e2e8f0; padding: 4px 8px;">${ctx.pools.length > 0 ? ctx.pools.map(escapeHtml).join(', ') : 'None'}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Epic Pool</td><td style="color: #e2e8f0; padding: 4px 8px;">${ctx.epicPool ? escapeHtml(ctx.epicPool) : 'None'}</td></tr>
        <tr><td style="color: #64748b; padding: 4px 8px;">Powers / Slots</td><td style="color: #e2e8f0; padding: 4px 8px;">${ctx.powerCount} / ${ctx.slotCount}</td></tr>
      </table>`;
  }

  let accountHtml = '';
  if (payload.userId || payload.userName) {
    accountHtml = `
      <h3 style="color: #94a3b8; margin-top: 16px;">Account</h3>
      <table style="width: 100%; border-collapse: collapse;">
        ${payload.userName ? `<tr><td style="color: #64748b; padding: 4px 8px;">Name</td><td style="color: #60a5fa; padding: 4px 8px;">${escapeHtml(payload.userName)}</td></tr>` : ''}
        ${payload.userId ? `<tr><td style="color: #64748b; padding: 4px 8px;">User ID</td><td style="color: #e2e8f0; padding: 4px 8px;"><code>${escapeHtml(payload.userId)}</code></td></tr>` : ''}
      </table>`;
  }

  let contactHtml = '';
  if (payload.globalName) {
    contactHtml = `
      <h3 style="color: #94a3b8; margin-top: 16px;">Global Name</h3>
      <p style="color: #60a5fa;">${escapeHtml(payload.globalName)}</p>`;
  }

  let snapshotHtml = '';
  if (payload.buildSnapshot) {
    snapshotHtml = `
      <h3 style="color: #94a3b8; margin-top: 16px;">Build Snapshot</h3>
      <p style="color: #60a5fa; font-size: 13px;">Attached as .json file (build + diagnostics)</p>`;
  }

  let diagnosticsHtml = '';
  if (payload.diagnostics) {
    diagnosticsHtml = buildDiagnosticsHtml(payload.diagnostics);
  }

  return `
    <div style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 600px; margin: 0 auto;">
      <div style="background: ${color}; color: white; padding: 12px 20px; border-radius: 8px 8px 0 0;">
        <h2 style="margin: 0; font-size: 18px;">${label}</h2>
      </div>
      <div style="background: #1e293b; color: #e2e8f0; padding: 20px; border-radius: 0 0 8px 8px;">
        <h3 style="color: #94a3b8; margin-top: 0;">Description</h3>
        <p style="white-space: pre-wrap; line-height: 1.5;">${escapeHtml(payload.description)}</p>
        ${accountHtml}
        ${contactHtml}
        ${contextHtml}
        ${diagnosticsHtml}
        ${snapshotHtml}
        <hr style="border: none; border-top: 1px solid #334155; margin: 16px 0;" />
        <p style="color: #64748b; font-size: 12px; margin-bottom: 0;">
          Sent: ${escapeHtml(payload.timestamp)}<br/>
          UA: ${escapeHtml(payload.userAgent)}
        </p>
      </div>
    </div>`;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const origin = request.headers.get('Origin');
    const corsHeaders = getCorsHeaders(origin);

    // CORS preflight
    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: corsHeaders });
    }

    if (request.method !== 'POST') {
      return new Response(JSON.stringify({ error: 'Method not allowed' }), {
        status: 405,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    // Validate origin in production — or accept the desktop build, which has no origin to send
    const isAllowed =
      (origin && ALLOWED_ORIGINS.some(o => origin.startsWith(o))) || isDesktopClient(request, env);
    if (!isAllowed) {
      return new Response(JSON.stringify({ error: 'Forbidden' }), {
        status: 403,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    let payload: FeedbackPayload;
    try {
      payload = await request.json();
    } catch {
      return new Response(JSON.stringify({ error: 'Invalid JSON' }), {
        status: 400,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    // Validate required fields
    if (!payload.description?.trim() || !payload.type) {
      return new Response(JSON.stringify({ error: 'Missing required fields' }), {
        status: 400,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    if (!['bug', 'suggestion', 'other'].includes(payload.type)) {
      return new Response(JSON.stringify({ error: 'Invalid feedback type' }), {
        status: 400,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }

    // Truncate description to prevent abuse
    const description = payload.description.substring(0, 5000);
    // Remove newlines from subject (email subjects must be single-line)
    const subjectPreview = description.substring(0, 60).replace(/[\r\n]+/g, ' ').trim();
    const subject = `[CoH Planner] ${payload.type}: ${subjectPreview}${description.length > 60 ? '...' : ''}`;

    try {
      // Build the email payload
      const emailPayload: Record<string, unknown> = {
        from: 'CoH Planner <onboarding@resend.dev>',
        to: env.FEEDBACK_EMAIL,
        subject,
        html: buildEmailHtml({ ...payload, description }),
      };

      // Attach build snapshot as a .json file if included
      if (payload.buildSnapshot) {
        // Generate a filename from the build name or timestamp
        let buildName = 'build';
        let parsedSnapshot: Record<string, unknown> | null = null;
        try {
          parsedSnapshot = JSON.parse(payload.buildSnapshot);
          const buildObj = parsedSnapshot && (parsedSnapshot.build as { name?: string } | undefined);
          if (buildObj?.name) {
            buildName = buildObj.name.replace(/[^a-zA-Z0-9_-]/g, '_').substring(0, 40);
          }
        } catch { /* use default */ }

        const timestamp = new Date().toISOString().slice(0, 10);
        const filename = `${buildName}_${timestamp}.json`;

        // Splice diagnostics into the attached JSON so triage has everything
        // in one file. The build importer ignores unknown top-level keys, so
        // re-importing the file still works.
        let prettyJson: string;
        if (parsedSnapshot) {
          if (payload.diagnostics) {
            parsedSnapshot.diagnostics = payload.diagnostics;
          }
          prettyJson = JSON.stringify(parsedSnapshot, null, 2);
        } else {
          prettyJson = payload.buildSnapshot;
        }

        // Resend accepts base64-encoded attachments
        const base64Content = btoa(unescape(encodeURIComponent(prettyJson)));
        emailPayload.attachments = [{
          filename,
          content: base64Content,
          type: 'application/json',
        }];
      }

      const resendResponse = await fetch('https://api.resend.com/emails', {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${env.RESEND_API_KEY}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(emailPayload),
      });

      if (!resendResponse.ok) {
        const error = await resendResponse.text();
        console.error('Resend error:', error);
        return new Response(JSON.stringify({ error: 'Failed to send feedback' }), {
          status: 502,
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      return new Response(JSON.stringify({ success: true }), {
        status: 200,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    } catch (err) {
      console.error('Worker error:', err);
      return new Response(JSON.stringify({ error: 'Internal error' }), {
        status: 500,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }
  },
};
