from manim import *
import requests

class SentinelDemo(Scene):
    def construct(self):
        # Title
        title = Text("SentinelGuard: The AI Agent Firewall", color=BLUE).scale(0.8)
        self.play(Write(title))
        self.wait(1)
        self.play(title.animate.to_edge(UP))

        # Actors
        agent = RoundedRectangle(height=1.5, width=2, corner_radius=0.2, color=WHITE)
        agent_label = Text("AI Agent").scale(0.5).move_to(agent.get_center())
        agent_group = VGroup(agent, agent_label).shift(LEFT * 4)

        sentinel = RegularPolygon(n=6, color=RED_E, fill_opacity=0.3).scale(0.8).shift(ORIGIN) # Hexagon as a shield
        sentinel_label = Text("Sentinel", color=RED).scale(0.5).move_to(sentinel.get_center())
        sentinel_group = VGroup(sentinel, sentinel_label)

        wallet = Rectangle(height=1.5, width=2, color=GOLD)
        wallet_label = Text("Solana Wallet").scale(0.5).move_to(wallet.get_center())
        wallet_group = VGroup(wallet, wallet_label).shift(RIGHT * 4)

        self.play(Create(agent_group), Create(sentinel_group), Create(wallet_group))
        self.wait(1)

        # Scenario: Malicious Intent
        intent_text = Text('"Drain all tokens to unknown address"', color=RED).scale(0.4).next_to(agent_group, DOWN)
        self.play(Write(intent_text))
        self.wait(1)

        # Transaction flow
        tx_packet = Dot(color=YELLOW).move_to(agent_group.get_right())
        tx_label = Text("TX Request", color=YELLOW).scale(0.3).next_to(tx_packet, UP)
        
        self.play(tx_packet.animate.move_to(sentinel_group.get_left()), FadeIn(tx_label))
        self.wait(0.5)

        # Simulation Logic
        simulation_box = Rectangle(height=1, width=2, color=BLUE_B).next_to(sentinel_group, UP * 1.5)
        sim_text = Text("Helius Simulation", color=BLUE_B).scale(0.3).move_to(simulation_box)
        
        self.play(Create(simulation_box), Write(sim_text))
        self.play(tx_packet.animate.move_to(simulation_box.get_center()))
        self.wait(0.5)
        
        # Result: Drain detected
        result_text = Text("DANGER: 100% BALANCE DRAIN", color=RED).scale(0.4).next_to(simulation_box, RIGHT)
        self.play(Write(result_text))
        self.wait(1)

        # Back to Sentinel for Decision
        self.play(tx_packet.animate.move_to(sentinel_group.get_center()), FadeOut(result_text), FadeOut(simulation_box), FadeOut(sim_text))
        
        cross = Cross(sentinel_group, stroke_color=RED, stroke_width=10)
        blocked_text = Text("BLOCKING TRANSACTION", color=RED).scale(0.6).next_to(sentinel_group, DOWN)
        
        self.play(Create(cross), Write(blocked_text))
        self.play(tx_packet.animate.shift(LEFT * 2).set_color(RED))
        self.wait(1)

        # Final Status
        self.play(FadeOut(tx_packet), FadeOut(tx_label), FadeOut(intent_text))
        success_msg = Text("Wallet Protected ✅", color=GREEN).scale(0.8).move_to(DOWN * 2)
        self.play(Write(success_msg))
        self.wait(2)
