--!strict
-- Prop Hunt — client HUD + disguise input.
-- Press E near a prop to disguise as it. Minimal HUD shows the current phase.

local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local UserInputService = game:GetService("UserInputService")
local Workspace = game:GetService("Workspace")

local Config = require(ReplicatedStorage:WaitForChild("PropHunt"):WaitForChild("Config"))
local Remotes = require(ReplicatedStorage.PropHunt.Remotes)

local player = Players.LocalPlayer
local stateChanged = Remotes.get("StateChanged") :: RemoteEvent
local requestDisguise = Remotes.get("RequestDisguise") :: RemoteEvent
local notify = Remotes.get("Notify") :: RemoteEvent

-- HUD ----------------------------------------------------------------------
local gui = Instance.new("ScreenGui")
gui.Name = "PropHuntHUD"
gui.ResetOnSpawn = false
gui.Parent = player:WaitForChild("PlayerGui")

local phaseLabel = Instance.new("TextLabel")
phaseLabel.AnchorPoint = Vector2.new(0.5, 0)
phaseLabel.Position = UDim2.new(0.5, 0, 0.02, 0)
phaseLabel.Size = UDim2.new(0.4, 0, 0.06, 0)
phaseLabel.BackgroundTransparency = 0.4
phaseLabel.BackgroundColor3 = Color3.new(0, 0, 0)
phaseLabel.TextColor3 = Color3.new(1, 1, 1)
phaseLabel.TextScaled = true
phaseLabel.Font = Enum.Font.GothamBold
phaseLabel.Text = "Waiting…"
phaseLabel.Parent = gui

local toast = Instance.new("TextLabel")
toast.AnchorPoint = Vector2.new(0.5, 1)
toast.Position = UDim2.new(0.5, 0, 0.9, 0)
toast.Size = UDim2.new(0.4, 0, 0.05, 0)
toast.BackgroundTransparency = 0.5
toast.BackgroundColor3 = Color3.new(0, 0, 0)
toast.TextColor3 = Color3.fromRGB(255, 230, 120)
toast.TextScaled = true
toast.Font = Enum.Font.Gotham
toast.Text = ""
toast.Visible = false
toast.Parent = gui

local hint = Instance.new("TextLabel")
hint.AnchorPoint = Vector2.new(0.5, 1)
hint.Position = UDim2.new(0.5, 0, 0.82, 0)
hint.Size = UDim2.new(0.3, 0, 0.04, 0)
hint.BackgroundTransparency = 1
hint.TextColor3 = Color3.new(1, 1, 1)
hint.TextStrokeTransparency = 0.4
hint.TextScaled = true
hint.Font = Enum.Font.Gotham
hint.Text = ""
hint.Visible = false
hint.Parent = gui

local function showToast(message: string)
	toast.Text = message
	toast.Visible = true
	task.delay(3, function()
		if toast.Text == message then
			toast.Visible = false
		end
	end)
end

stateChanged.OnClientEvent:Connect(function(state)
	phaseLabel.Text = tostring(state.phase)
end)
notify.OnClientEvent:Connect(showToast)

-- Disguise targeting -------------------------------------------------------
local function nearestProp(): Model?
	local character = player.Character
	local root = character and character:FindFirstChild("HumanoidRootPart") :: BasePart?
	local folder = Workspace:FindFirstChild(Config.HIDEABLE_PROPS_FOLDER)
	if not root or not folder then
		return nil
	end
	local best: Model?, bestDist = nil, Config.DISGUISE_RANGE
	for _, prop in folder:GetChildren() do
		if prop:IsA("Model") and prop.PrimaryPart then
			local dist = (prop.PrimaryPart.Position - root.Position).Magnitude
			if dist <= bestDist then
				best, bestDist = prop, dist
			end
		end
	end
	return best
end

game:GetService("RunService").RenderStepped:Connect(function()
	local prop = nearestProp()
	if prop then
		hint.Text = "Press E to hide as " .. prop.Name
		hint.Visible = true
	else
		hint.Visible = false
	end
end)

UserInputService.InputBegan:Connect(function(input, processed)
	if processed then
		return
	end
	if input.KeyCode == Enum.KeyCode.E then
		local prop = nearestProp()
		if prop then
			requestDisguise:FireServer(prop.Name)
		end
	end
end)
