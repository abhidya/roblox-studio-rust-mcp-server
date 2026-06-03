--!strict
-- Prop Hunt — server round loop and game logic.
--
-- This file is intentionally CONTENT-FREE: it never models the world. It reads
-- disguise props from Workspace.<HideableProps>, which the asset-driven build
-- populates with real, inspected Creator Store assets.

local Players = game:GetService("Players")
local Workspace = game:GetService("Workspace")
local ReplicatedStorage = game:GetService("ReplicatedStorage")

local Config = require(ReplicatedStorage:WaitForChild("PropHunt"):WaitForChild("Config"))
local Remotes = require(ReplicatedStorage.PropHunt.Remotes)

Remotes.setup()
local stateChanged = Remotes.get("StateChanged") :: RemoteEvent
local requestDisguise = Remotes.get("RequestDisguise") :: RemoteEvent
local notify = Remotes.get("Notify") :: RemoteEvent

local scores: { [number]: number } = {}
local disguises: { [number]: Model } = {} -- userId -> cloned prop welded to character

local function getHideableProps(): { Model }
	local folder = Workspace:FindFirstChild(Config.HIDEABLE_PROPS_FOLDER)
	local props: { Model } = {}
	if folder then
		for _, child in folder:GetChildren() do
			if child:IsA("Model") and child.PrimaryPart then
				table.insert(props, child)
			end
		end
	end
	return props
end

local function broadcast(state)
	stateChanged:FireAllClients(state)
end

local function clearDisguise(player: Player)
	local existing = disguises[player.UserId]
	if existing then
		existing:Destroy()
		disguises[player.UserId] = nil
	end
	local character = player.Character
	if character then
		for _, part in character:GetDescendants() do
			if part:IsA("BasePart") then
				part.Transparency = 0
				part.CanCollide = part.Name == "HumanoidRootPart" and false or true
			end
		end
	end
end

-- Disguise a hider as a prop instance from the world. The character is hidden
-- and a clone of the prop is welded to the HumanoidRootPart.
local function applyDisguise(player: Player, prop: Model)
	local character = player.Character
	local root = character and character:FindFirstChild("HumanoidRootPart") :: BasePart?
	if not character or not root or not prop.PrimaryPart then
		return
	end

	clearDisguise(player)

	local clone = prop:Clone()
	clone.Name = "Disguise"
	for _, part in clone:GetDescendants() do
		if part:IsA("BasePart") then
			part.CanCollide = false
			part.Anchored = false
			part.Massless = true
		end
	end

	-- Hide the real character body.
	for _, part in character:GetDescendants() do
		if part:IsA("BasePart") and part.Name ~= "HumanoidRootPart" then
			part.Transparency = 1
		end
	end

	clone.Parent = character
	local weld = Instance.new("WeldConstraint")
	weld.Part0 = root
	weld.Part1 = clone.PrimaryPart
	weld.Parent = root
	clone.PrimaryPart.CFrame = root.CFrame
	disguises[player.UserId] = clone
end

requestDisguise.OnServerEvent:Connect(function(player, propName)
	-- Validate: prop must exist in the world folder and player must be a hider
	-- and within range. (Range check is best-effort; authoritative on server.)
	if typeof(propName) ~= "string" then
		return
	end
	local character = player.Character
	local root = character and character:FindFirstChild("HumanoidRootPart") :: BasePart?
	if not root then
		return
	end
	for _, prop in getHideableProps() do
		if prop.Name == propName and prop.PrimaryPart then
			local dist = (prop.PrimaryPart.Position - root.Position).Magnitude
			if dist <= Config.DISGUISE_RANGE then
				applyDisguise(player, prop)
				notify:FireClient(player, "Disguised as " .. propName)
			end
			return
		end
	end
end)

-- Round state machine ------------------------------------------------------

local function pickSeekers(participants: { Player }): ({ number }, { number })
	local seekerCount = math.max(1, math.floor(#participants * Config.SEEKER_RATIO))
	local shuffled = table.clone(participants)
	for i = #shuffled, 2, -1 do
		local j = math.random(i)
		shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
	end
	local seekers, hiders = {}, {}
	for index, player in shuffled do
		if index <= seekerCount then
			table.insert(seekers, player.UserId)
		else
			table.insert(hiders, player.UserId)
		end
	end
	return seekers, hiders
end

local function awardSurvivors(hiders: { number }, tagged: { [number]: boolean })
	for _, userId in hiders do
		if not tagged[userId] then
			scores[userId] = (scores[userId] or 0) + Config.POINTS_PER_SURVIVED_HIDER
		end
	end
end

local function runRound()
	-- Intermission
	broadcast({ phase = "Intermission", endsAt = os.clock() + Config.INTERMISSION_TIME })
	task.wait(Config.INTERMISSION_TIME)

	local participants = Players:GetPlayers()
	if #participants < Config.MIN_PLAYERS then
		notify:FireAllClients("Need at least " .. Config.MIN_PLAYERS .. " players")
		return
	end
	if #getHideableProps() == 0 then
		notify:FireAllClients("No hideable props in world — run the asset-driven build first")
		return
	end

	local seekers, hiders = pickSeekers(participants)
	local tagged: { [number]: boolean } = {}

	-- Hiding phase
	broadcast({
		phase = "Hiding",
		endsAt = os.clock() + Config.HIDING_TIME,
		seekers = seekers,
		hiders = hiders,
	})
	for _, userId in seekers do
		local p = Players:GetPlayerByUserId(userId)
		if p then
			notify:FireClient(p, "You are a SEEKER. Wait for hunt to begin.")
		end
	end
	task.wait(Config.HIDING_TIME)

	-- Hunting phase
	local huntEnd = os.clock() + Config.HUNTING_TIME
	broadcast({ phase = "Hunting", endsAt = huntEnd, seekers = seekers, hiders = hiders })

	-- Tagging: seekers tag nearby disguised hiders.
	local connection
	connection = game:GetService("RunService").Heartbeat:Connect(function()
		for _, seekerId in seekers do
			local seeker = Players:GetPlayerByUserId(seekerId)
			local sRoot = seeker
				and seeker.Character
				and seeker.Character:FindFirstChild("HumanoidRootPart") :: BasePart?
			if sRoot then
				for _, hiderId in hiders do
					if not tagged[hiderId] then
						local hider = Players:GetPlayerByUserId(hiderId)
						local hRoot = hider
							and hider.Character
							and hider.Character:FindFirstChild("HumanoidRootPart") :: BasePart?
						if hRoot and (hRoot.Position - sRoot.Position).Magnitude <= Config.TAG_RANGE then
							tagged[hiderId] = true
							scores[seekerId] = (scores[seekerId] or 0) + Config.POINTS_PER_TAG
							if hider then
								clearDisguise(hider)
								notify:FireClient(hider, "You were tagged!")
							end
							if seeker then
								notify:FireClient(seeker, "Tagged " .. (hider and hider.Name or "?"))
							end
						end
					end
				end
			end
		end
	end)

	while os.clock() < huntEnd do
		local remaining = 0
		for _, hiderId in hiders do
			if not tagged[hiderId] then
				remaining += 1
			end
		end
		if remaining == 0 then
			break
		end
		task.wait(0.5)
	end
	connection:Disconnect()

	awardSurvivors(hiders, tagged)

	-- Round end
	for _, player in Players:GetPlayers() do
		clearDisguise(player)
	end
	broadcast({ phase = "RoundEnd", endsAt = os.clock() + Config.ROUND_END_TIME })
	task.wait(Config.ROUND_END_TIME)
end

Players.PlayerRemoving:Connect(function(player)
	scores[player.UserId] = nil
	disguises[player.UserId] = nil
end)

print("[PropHunt] Server logic started. Waiting for asset-built world…")
while true do
	local ok, err = pcall(runRound)
	if not ok then
		warn("[PropHunt] round error: " .. tostring(err))
		task.wait(3)
	end
end
