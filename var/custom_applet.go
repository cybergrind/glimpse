package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type message struct {
	Type string      `json:"type"`
	Data interface{} `json:"data"`
}

type callbackMessage struct {
	Type string `json:"type"`
	Data struct {
		ID     string      `json:"id"`
		Event  string      `json:"event"`
		Button string      `json:"button,omitempty"`
		Text   string      `json:"text,omitempty"`
		Value  interface{} `json:"value,omitempty"`
		DeltaY *float64    `json:"delta_y,omitempty"`
	} `json:"data"`
}

type initMessage struct {
	Type string `json:"type"`
	Data struct {
		Instance string `json:"instance"`
	} `json:"data"`
}

type iconSource struct {
	Type  string `json:"type"`
	Value string `json:"value"`
}

type statusItem struct {
	ID   string      `json:"id,omitempty"`
	Icon *iconSource `json:"icon,omitempty"`
	Text string      `json:"text,omitempty"`
}

type heroData struct {
	Icon     *iconSource `json:"icon,omitempty"`
	Title    string      `json:"title"`
	Subtitle string      `json:"subtitle"`
}

type state struct {
	instance       string
	environment    string
	version        string
	approver       string
	token          string
	requireChecks  bool
	freeze         bool
	rollout        float64
	status         string
	region         string
	lastCallback   string
	lastStatusTap  string
	logLines       []string
	externalAsset  string
	currentEnvIcon string
}

func main() {
	assetPath := filepath.Join(mustGetwd(), "var", "custom_applet_asset.svg")
	ensureAsset(assetPath)

	s := state{
		environment:    "staging",
		version:        "2026.04.06-rc1",
		approver:       "alex",
		requireChecks:  true,
		freeze:         false,
		rollout:        0.25,
		status:         "Ready",
		region:         "eu-central",
		externalAsset:  assetPath,
		currentEnvIcon: "software-update-available-symbolic",
		logLines: []string{
			"Queued release candidate 2026.04.06-rc1",
			"Smoke tests green",
			"Waiting for operator action",
		},
	}

	emitAll(s)

	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		line := scanner.Bytes()

		var initMsg initMessage
		if err := json.Unmarshal(line, &initMsg); err == nil && initMsg.Type == "init" {
			s.instance = initMsg.Data.Instance
			appendLog(&s, "Connected to panel instance "+s.instance)
			emitAll(s)
			continue
		}

		var cb callbackMessage
		if err := json.Unmarshal(line, &cb); err != nil || cb.Type != "callback" {
			continue
		}

		s.lastCallback = cb.Data.ID + ":" + cb.Data.Event

		switch cb.Data.ID {
		case "deploy_status":
			handleStatusCallback(&s, cb)
		case "environment":
			if cb.Data.Event == "change" {
				s.environment = decodeDropdownID(cb.Data.Value, s.environment)
				s.currentEnvIcon = environmentIcon(s.environment)
				appendLog(&s, "Environment changed to "+s.environment)
			}
		case "version":
			if cb.Data.Event == "input" {
				s.version = strings.TrimSpace(cb.Data.Text)
				if s.version == "" {
					s.version = "draft"
				}
			}
		case "approver":
			if cb.Data.Event == "input" {
				s.approver = cb.Data.Text
			}
		case "token":
			if cb.Data.Event == "input" {
				s.token = cb.Data.Text
			}
		case "require_checks":
			if cb.Data.Event == "toggle" {
				s.requireChecks = decodeBool(cb.Data.Value)
				appendLog(&s, boolMessage(s.requireChecks, "Checks required", "Checks bypassed"))
			}
		case "freeze":
			if cb.Data.Event == "toggle" {
				s.freeze = decodeBool(cb.Data.Value)
				appendLog(&s, boolMessage(s.freeze, "Deploy freeze enabled", "Deploy freeze cleared"))
			}
		case "rollout":
			if cb.Data.Event == "change" {
				s.rollout = clamp(decodeFloat(cb.Data.Value, s.rollout), 0.0, 1.0)
				appendLog(&s, fmt.Sprintf("Rollout set to %.0f%%", s.rollout*100))
			}
		case "deploy_now":
			if cb.Data.Event == "click" {
				s.status = "Deploying"
				appendLog(&s, fmt.Sprintf("Deploy started for %s on %s", s.version, s.environment))
			}
		case "rollback":
			if cb.Data.Event == "click" {
				s.status = "Rollback queued"
				appendLog(&s, "Rollback queued for previous stable release")
			}
		case "clear_log":
			if cb.Data.Event == "click" {
				s.logLines = []string{"Activity log cleared"}
			}
		}

		emitAll(s)
	}
}

func emitAll(s state) {
	emitStatus(s)
	emitHero(s)
	emitTree(s)
}

func emitStatus(s state) {
	mustEmit(message{
		Type: "status",
		Data: map[string]interface{}{
			"items": []statusItem{
				{
					ID: "deploy_status",
					Icon: &iconSource{
						Type:  "name",
						Value: s.currentEnvIcon,
					},
					Text: fmt.Sprintf("%s %.0f%%", s.status, s.rollout*100),
				},
				{
					ID: "release_badge",
					Icon: &iconSource{
						Type:  "name",
						Value: "emblem-ok-symbolic",
					},
					Text: shortVersion(s.version),
				},
			},
		},
	})
}

func emitHero(s state) {
	mustEmit(message{
		Type: "hero",
		Data: heroData{
			Icon: &iconSource{
				Type:  "name",
				Value: environmentIcon(s.environment),
			},
			Title:    fmt.Sprintf("%s deploy", strings.Title(s.environment)),
			Subtitle: fmt.Sprintf("%s · %s · %.0f%% rollout", s.status, s.region, s.rollout*100),
		},
	})
}

func emitTree(s state) {
	mustEmit(message{
		Type: "tree",
		Data: map[string]interface{}{
			"content": map[string]interface{}{
				"type": "box",
				"data": map[string]interface{}{
					"orientation": "vertical",
					"spacing":     12,
					"children": []interface{}{
						boxRow(
							imageNode("file", &iconSource{Type: "path", Value: s.externalAsset}),
							labelNode("Release control"),
						),
						separatorNode(),
						gridNode([]map[string]interface{}{
							gridCell(0, 0, labelNode("Version")),
							gridCell(0, 1, labelNode(s.version)),
							gridCell(1, 0, labelNode("Approver")),
							gridCell(1, 1, labelNode(emptyFallback(s.approver, "unassigned"))),
							gridCell(2, 0, labelNode("Last callback")),
							gridCell(2, 1, labelNode(emptyFallback(s.lastCallback, "none yet"))),
							gridCell(3, 0, labelNode("Status tap")),
							gridCell(3, 1, labelNode(emptyFallback(s.lastStatusTap, "no panel click yet"))),
						}),
						separatorNode(),
						map[string]interface{}{
							"type": "dropdown",
							"data": map[string]interface{}{
								"id": "environment",
								"items": []map[string]interface{}{
									{"id": "staging", "label": "Staging"},
									{"id": "production", "label": "Production"},
									{"id": "canary", "label": "Canary"},
								},
								"selected": envIndex(s.environment),
							},
						},
						map[string]interface{}{
							"type": "entry",
							"data": map[string]interface{}{
								"id":          "version",
								"placeholder": "Release version",
								"text":        s.version,
							},
						},
						map[string]interface{}{
							"type": "entry",
							"data": map[string]interface{}{
								"id":          "approver",
								"placeholder": "Approver",
								"text":        s.approver,
							},
						},
						map[string]interface{}{
							"type": "password",
							"data": map[string]interface{}{
								"id":          "token",
								"placeholder": "Approval token",
								"text":        s.token,
							},
						},
						map[string]interface{}{
							"type": "checkbox",
							"data": map[string]interface{}{
								"id":     "require_checks",
								"label":  "Require smoke checks",
								"active": s.requireChecks,
							},
						},
						map[string]interface{}{
							"type": "switch",
							"data": map[string]interface{}{
								"id":     "freeze",
								"label":  "Freeze deploys",
								"active": s.freeze,
							},
						},
						map[string]interface{}{
							"type": "scale",
							"data": map[string]interface{}{
								"id":          "rollout",
								"min":         0.0,
								"max":         1.0,
								"step":        0.05,
								"value":       s.rollout,
								"orientation": "horizontal",
							},
						},
						boxRow(
							buttonNode("deploy_now", "Deploy now"),
							buttonNode("rollback", "Rollback"),
							buttonNode("clear_log", "Clear log"),
						),
						map[string]interface{}{
							"type": "scroll",
							"data": map[string]interface{}{
								"child": map[string]interface{}{
									"type": "box",
									"data": map[string]interface{}{
										"orientation": "vertical",
										"spacing":     6,
										"children":    logNodes(s.logLines),
									},
								},
							},
						},
					},
				},
			},
		},
	})
}

func handleStatusCallback(s *state, cb callbackMessage) {
	switch cb.Data.Event {
	case "click":
		s.lastStatusTap = cb.Data.Button
		if cb.Data.Button == "left" {
			appendLog(s, "Primary status item clicked")
		} else {
			appendLog(s, "Status item clicked with "+emptyFallback(cb.Data.Button, "unknown"))
		}
	case "scroll":
		delta := 0.05
		if cb.Data.DeltaY != nil && *cb.Data.DeltaY > 0 {
			s.rollout = clamp(s.rollout-delta, 0.0, 1.0)
		} else {
			s.rollout = clamp(s.rollout+delta, 0.0, 1.0)
		}
		appendLog(s, fmt.Sprintf("Rollout adjusted from panel to %.0f%%", s.rollout*100))
	}
}

func environmentIcon(environment string) string {
	switch environment {
	case "production":
		return "network-server-symbolic"
	case "canary":
		return "weather-few-clouds-symbolic"
	default:
		return "software-update-available-symbolic"
	}
}

func shortVersion(version string) string {
	if len(version) <= 10 {
		return version
	}
	return version[:10]
}

func envIndex(environment string) int {
	switch environment {
	case "production":
		return 1
	case "canary":
		return 2
	default:
		return 0
	}
}

func boxRow(children ...interface{}) map[string]interface{} {
	return map[string]interface{}{
		"type": "box",
		"data": map[string]interface{}{
			"orientation": "horizontal",
			"spacing":     8,
			"children":    children,
		},
	}
}

func buttonNode(id, label string) map[string]interface{} {
	return map[string]interface{}{
		"type": "button",
		"data": map[string]interface{}{
			"id":    id,
			"label": label,
		},
	}
}

func imageNode(id string, icon *iconSource) map[string]interface{} {
	return map[string]interface{}{
		"type": "image",
		"data": map[string]interface{}{
			"id":         id,
			"icon":       icon,
			"pixel_size": 28,
		},
	}
}

func labelNode(text string) map[string]interface{} {
	return map[string]interface{}{
		"type": "label",
		"data": map[string]interface{}{
			"text": text,
		},
	}
}

func separatorNode() map[string]interface{} {
	return map[string]interface{}{
		"type": "separator",
		"data": map[string]interface{}{
			"orientation": "horizontal",
		},
	}
}

func gridNode(children []map[string]interface{}) map[string]interface{} {
	return map[string]interface{}{
		"type": "grid",
		"data": map[string]interface{}{
			"row_spacing":    6,
			"column_spacing": 12,
			"children":       children,
		},
	}
}

func gridCell(row, column int, child map[string]interface{}) map[string]interface{} {
	return map[string]interface{}{
		"row":    row,
		"column": column,
		"child":  child,
	}
}

func logNodes(lines []string) []interface{} {
	nodes := make([]interface{}, 0, len(lines))
	for _, line := range lines {
		nodes = append(nodes, labelNode(line))
	}
	return nodes
}

func appendLog(s *state, line string) {
	s.logLines = append([]string{line}, s.logLines...)
	if len(s.logLines) > 8 {
		s.logLines = s.logLines[:8]
	}
}

func decodeBool(value interface{}) bool {
	boolean, ok := value.(bool)
	return ok && boolean
}

func decodeFloat(value interface{}, fallback float64) float64 {
	switch v := value.(type) {
	case float64:
		return v
	case int:
		return float64(v)
	default:
		return fallback
	}
}

func decodeDropdownID(value interface{}, fallback string) string {
	payload, ok := value.(map[string]interface{})
	if !ok {
		return fallback
	}
	if id, ok := payload["id"].(string); ok && id != "" {
		return id
	}
	return fallback
}

func boolMessage(active bool, on, off string) string {
	if active {
		return on
	}
	return off
}

func clamp(value, min, max float64) float64 {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}

func emptyFallback(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func ensureAsset(path string) {
	if _, err := os.Stat(path); err == nil {
		return
	}
	content := `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<rect width="64" height="64" rx="12" fill="#1f2937"/>
<rect x="10" y="12" width="44" height="12" rx="6" fill="#60a5fa"/>
<rect x="10" y="30" width="24" height="24" rx="6" fill="#34d399"/>
<rect x="38" y="30" width="16" height="24" rx="6" fill="#f59e0b"/>
</svg>
`
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		fatal(err)
	}
}

func mustGetwd() string {
	dir, err := os.Getwd()
	if err != nil {
		fatal(err)
	}
	return dir
}

func mustEmit(msg message) {
	if err := json.NewEncoder(os.Stdout).Encode(msg); err != nil {
		fatal(err)
	}
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}
