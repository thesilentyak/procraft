"""Makes the starter workflows for ProCraft Diffusion: each in ComfyUI's regular (UI) format, to
open in ComfyUI so its tab sends it to the app, and in the API format, as the app reads it (a test
fixture). One description per workflow; both files come from it."""

import json
import sys

# A node: id, class, widget values (in the node's widget order), inputs fed by links
# ({input name: (from node, from slot)}), outputs (name, type), and which inputs are widgets
# given a link (their widget names, so the regular format marks them).
MODEL = ("z_image_turbo_bf16.safetensors", "qwen_3_4b.safetensors", "ae.safetensors")


def starter(paint_over: bool):
    unet, te, vae = MODEL
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(2, "ModelSamplingAuraFlow", [3], {"model": (1, 0)}, [("MODEL", "MODEL")],
         api_widgets={"shift": 3}, pos=(340, 0))
    node(3, "CLIPLoader", [te, "lumina2", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "lumina2", "device": "default"}, pos=(0, 140))
    node(4, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    node(5, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(6, "CLIPTextEncode", [""], {"clip": (3, 0), "text": (5, 0)}, [("CONDITIONING", "CONDITIONING")],
         widget_links=["text"], pos=(340, 140))
    node(7, "ConditioningZeroOut", [], {"conditioning": (6, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(700, 140))
    node(8, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(9, "ImageScaleToTotalPixels", ["lanczos", 1.0, 16], {"image": (8, 0)}, [("IMAGE", "IMAGE")],
         api_widgets={"upscale_method": "lanczos", "megapixels": 1.0, "resolution_steps": 16}, pos=(340, 600))
    node(10, "VAEEncode", [], {"pixels": (9, 0), "vae": (4, 0)}, [("LATENT", "LATENT")], pos=(700, 600))
    latent = (10, 0)
    denoise = 1.0
    if paint_over:
        node(11, "ETN_KritaSelection", ["automatic", 32], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
             api_widgets={"context": "automatic", "padding": 32}, title="Selection (from the app)", pos=(340, 760))
        node(12, "SetLatentNoiseMask", [], {"samples": (10, 0), "mask": (11, 0)}, [("LATENT", "LATENT")], pos=(1000, 600))
        latent = (12, 0)
        node(13, "ETN_Parameter", ["Strength", "number", "0.6", 0.05, 1.0], {}, [("value", "*")],
             api_widgets={"name": "Strength", "type": "number", "default": "0.6", "min": 0.05, "max": 1.0},
             title="Strength (from the app)", pos=(700, 400))
    sampler_links = {"model": (2, 0), "positive": (6, 0), "negative": (7, 0), "latent_image": latent, "seed": (8, 3)}
    sampler_widget_links = ["seed"]
    api = {"steps": 8, "cfg": 1, "sampler_name": "res_multistep", "scheduler": "simple"}
    if paint_over:
        sampler_links["denoise"] = (13, 0)
        sampler_widget_links.append("denoise")
    else:
        api["denoise"] = denoise
    node(14, "KSampler", [0, "fixed", 8, 1, "res_multistep", "simple", denoise], sampler_links, [("LATENT", "LATENT")],
         widget_links=sampler_widget_links, api_widgets=api, pos=(1300, 140))
    node(15, "VAEDecode", [], {"samples": (14, 0), "vae": (4, 0)}, [("IMAGE", "IMAGE")], pos=(1650, 140))
    node(16, "ETN_KritaOutput", [0, 0, "", "default", False], {"images": (15, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(1900, 140))
    return nodes


def minimax(edit: bool):
    """MiniMax H3 Image: a new picture at about 4 MP in the canvas's shape (the model is tuned for
    2048 x 2048 of area), or an edit of the canvas (Picture 1, its selection's part when there is
    one), the prompt an instruction. As ComfyUI's own templates, without their prompt upsampler."""
    unet = "minimax_h3_image_pruned_int8_convrot.safetensors"
    te = "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"
    vae = "minimax_h3_image_vae_fp16.safetensors"
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(2, "CLIPLoader", [te, "minimax", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "minimax", "device": "default"}, pos=(0, 140))
    node(3, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    node(4, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(5, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    links = {"clip": (2, 0), "vae": (3, 0)}
    widget_links = ["prompt"]
    if edit:
        node(6, "ETN_KritaSelection", ["automatic", 64], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
             api_widgets={"context": "automatic", "padding": 64}, title="Selection (from the app)", pos=(0, 780))
        node(7, "StringConcatenate", ["Edit <Picture 1>: ", "", ""], {"string_b": (4, 0)}, [("STRING", "STRING")],
             widget_links=["string_b"], api_widgets={"string_a": "Edit <Picture 1>: ", "delimiter": ""}, pos=(340, 420))
        links.update({"prompt": (7, 0), "images.image_1": (5, 0)})
        api = {"width": 0, "height": 0}
    else:
        node(7, "ImageScaleToTotalPixels", ["lanczos", 4.0, 32], {"image": (5, 0)}, [("IMAGE", "IMAGE")],
             api_widgets={"upscale_method": "lanczos", "megapixels": 4.0, "resolution_steps": 32}, pos=(340, 600))
        node(8, "GetImageSize", [], {"image": (7, 0)}, [("width", "INT"), ("height", "INT"), ("batch_size", "INT")], pos=(700, 600))
        links.update({"prompt": (4, 0), "width": (8, 0), "height": (8, 1)})
        widget_links += ["width", "height"]
        api = {}
    node(9, "MiniMaxH3ImageConditioning", ["", 0, 0], links, [("positive", "CONDITIONING"), ("LATENT", "LATENT")],
         widget_links=widget_links, api_widgets=api, pos=(700, 140))
    node(10, "ConditioningZeroOut", [], {"conditioning": (9, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(1050, 300))
    node(11, "KSampler", [0, "fixed", 6, 1, "euler", "simple", 1],
         {"model": (1, 0), "positive": (9, 0), "negative": (10, 0), "latent_image": (9, 1), "seed": (5, 3)},
         [("LATENT", "LATENT")], widget_links=["seed"],
         api_widgets={"steps": 6, "cfg": 1, "sampler_name": "euler", "scheduler": "simple", "denoise": 1}, pos=(1300, 140))
    node(12, "VAEDecode", [], {"samples": (11, 0), "vae": (3, 0)}, [("IMAGE", "IMAGE")], pos=(1650, 140))
    node(13, "ETN_KritaOutput", [0, 0, "", "default", False], {"images": (12, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(1900, 140))
    return nodes


def rd_flow():
    """RD Flow 64: 64 x 64 pixel art (Retro Diffusion's node pack, rd-flow-64 in custom_nodes; its
    weights download on the first run). The model wants a long description, so Qwen3-VL 4B writes
    one from the prompt with the pack's own instructions. The sprite is scaled up with hard edges and
    padded to the canvas, transparent round it."""
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(2, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 0))
    node(3, "ETN_Parameter", ["Background", "choice", "transparent", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Background", "type": "choice", "default": "transparent", "min": -1e10, "max": 1e10},
         title="Background (from the app)", pos=(0, 200))
    node(4, "ETN_Parameter", ["Palette", "text", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Palette", "type": "text", "default": "", "min": -1e10, "max": 1e10},
         title="Palette: hex colours, or empty (from the app)", pos=(0, 400))
    node(5, "RDFlow64SystemPrompt", ["", "transparent", 0, ""],
         {"request": (2, 0), "background": (3, 0), "seed": (1, 3), "palette": (4, 0)},
         [("system_prompt", "STRING"), ("user_message", "STRING")],
         widget_links=["request", "background", "seed", "palette"], pos=(380, 0))
    node(6, "CLIPLoader", ["qwen3vl_4b_fp8_scaled.safetensors", "krea2", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": "qwen3vl_4b_fp8_scaled.safetensors", "type": "krea2", "device": "default"},
         pos=(380, 260))
    node(7, "TextGenerateV2", ["", 700, "on", 0.6, 20, 0.95, 0, 1.05, 0, 0, False, True, "auto"],
         {"clip": (6, 0), "system_prompt": (5, 0), "prompt": (5, 1)},
         [("generated_text", "STRING"), ("thinking", "STRING")], widget_links=["prompt"],
         api_widgets={"max_length": 700, "sampling_mode": "on", "sampling_mode.temperature": 0.6,
                      "sampling_mode.top_k": 20, "sampling_mode.top_p": 0.95, "sampling_mode.min_p": 0,
                      "sampling_mode.repetition_penalty": 1.05, "sampling_mode.presence_penalty": 0,
                      "sampling_mode.seed": 0, "thinking": False, "use_default_template": True, "mtp": "auto"},
         title="Writes the long description", pos=(760, 0))
    node(8, "RDFlow64CleanCaption", ["transparent"], {"llm_output": (7, 0), "background": (3, 0)},
         [("caption", "STRING")], widget_links=["background"], pos=(1140, 0))
    node(9, "RDFlow64Loader", ["cpu", "auto", "auto", ""], {}, [("rd_flow_64", "RDFLOW64")],
         api_widgets={"text_device": "cpu", "model_device": "auto", "precision": "auto", "weights_path": ""},
         title="RD Flow 64 (its prompt reader on the CPU, leaving the GPU to ComfyUI)", pos=(1140, 260))
    node(10, "RDFlow64Generate", ["", 0, "fixed", 1, "transparent", 16, 4.0, 1, ""],
         {"rd_flow_64": (9, 0), "caption": (8, 0), "seed": (1, 3), "background": (3, 0), "palette": (4, 0)},
         [("image", "IMAGE"), ("mask", "MASK"), ("palette", "STRING"), ("rgba", "IMAGE")],
         widget_links=["caption", "seed", "background", "palette"],
         api_widgets={"batch_size": 1, "steps": 16, "guidance": 4.0, "scale": 1}, pos=(1500, 0))
    pad = {"padding_color": "black", "interpolation": "nearest-exact"}
    node(11, "ResizeAndPadImage", [512, 512, "black", "nearest-exact"],
         {"image": (10, 0), "target_width": (1, 1), "target_height": (1, 2)}, [("IMAGE", "IMAGE")],
         widget_links=["target_width", "target_height"], api_widgets=pad, title="Sharp pixels, fitted to the canvas",
         pos=(1880, 0))
    node(12, "MaskToImage", [], {"mask": (10, 1)}, [("IMAGE", "IMAGE")], pos=(1880, 260))
    node(13, "ResizeAndPadImage", [512, 512, "white", "nearest-exact"],
         {"image": (12, 0), "target_width": (1, 1), "target_height": (1, 2)}, [("IMAGE", "IMAGE")],
         widget_links=["target_width", "target_height"], api_widgets=dict(pad, padding_color="white"),
         title="Transparent round it", pos=(2200, 260))
    node(14, "ImageToMask", ["red"], {"image": (13, 0)}, [("MASK", "MASK")], api_widgets={"channel": "red"},
         pos=(2520, 260))
    node(15, "JoinImageWithAlpha", [], {"image": (11, 0), "alpha": (14, 0)}, [("IMAGE", "IMAGE")], pos=(2520, 0))
    node(16, "ETN_KritaOutput", [0, 0, "Pixel art", "default", False], {"images": (15, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "Pixel art", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(2840, 0))
    return nodes


def widget_inputs(n):
    # the inputs that are widgets given a link
    types = {"text": "STRING", "seed": "INT", "denoise": "FLOAT", "prompt": "STRING", "width": "INT",
             "height": "INT", "string_b": "STRING", "request": "STRING", "background": "COMBO",
             "palette": "STRING", "caption": "STRING", "target_width": "INT", "target_height": "INT"}
    return [(w, types.get(w, "*")) for w in n["widget_links"]]


def to_ui(nodes, name):
    links = []
    ui_nodes = []
    link_id = 0
    by_id = {n["id"]: n for n in nodes}
    out_links = {}
    for n in nodes:
        for inp, (src, slot) in n["links"].items():
            link_id += 1
            typ = by_id[src]["outputs"][slot][1]
            links.append([link_id, src, slot, n["id"], inp, typ])
            out_links.setdefault((src, slot), []).append(link_id)
    for order, n in enumerate(nodes):
        inputs = []
        slot = 0
        index = {}
        for inp, (src, s) in n["links"].items():
            lid = next(l[0] for l in links if l[3] == n["id"] and l[4] == inp)
            entry = {"name": inp, "type": by_id[src]["outputs"][s][1], "link": lid}
            if inp in n["widget_links"]:
                entry["widget"] = {"name": inp}
                entry["type"] = dict(widget_inputs(n)).get(inp, "*")
            inputs.append(entry)
            index[inp] = slot
            slot += 1
        outputs = [{"name": oname, "type": otype, "links": out_links.get((n["id"], i), []), "slot_index": i}
                   for i, (oname, otype) in enumerate(n["outputs"])]
        ui = {"id": n["id"], "type": n["cls"], "pos": list(n["pos"]), "size": [300, 100], "flags": {}, "order": order,
              "mode": 0, "inputs": inputs, "outputs": outputs, "properties": {"Node name for S&R": n["cls"]},
              "widgets_values": n["widgets"]}
        if n["title"]:
            ui["title"] = n["title"]
        ui_nodes.append(ui)
    # links refer to target slots by index
    for l in links:
        target = next(x for x in ui_nodes if x["id"] == l[3])
        l[4] = next(i for i, e in enumerate(target["inputs"]) if e["name"] == l[4])
    note = {"id": 99, "type": "MarkdownNote", "pos": [0, -260], "size": [620, 220], "flags": {}, "order": len(ui_nodes),
            "mode": 0, "inputs": [], "outputs": [], "properties": {},
            "widgets_values": [f"## {name}\n\nMade for ProCraft Diffusion. Keep this workflow **open in ComfyUI**: the app on your phone gets it as the **ComfyUI (live)** card. On the phone, hold that card and choose **Keep** to keep it for good.\n\nThe nodes marked *(from the app)* are filled in by the app: your canvas, your prompt, the selection. **Back to the app** sends the result to your phone. Pressing Run here does nothing useful."]}
    ui_nodes.append(note)
    return {"id": "procraft-" + name.lower().replace(" ", "-"), "revision": 0, "last_node_id": 99,
            "last_link_id": link_id, "nodes": ui_nodes, "links": links, "groups": [], "config": {},
            "extra": {"ds": {"scale": 0.7, "offset": [60, 320]}}, "version": 0.4}


def to_api(nodes):
    out = {}
    for n in nodes:
        inputs = dict(n["api"])
        if n["cls"] == "CLIPTextEncode":
            pass
        for inp, (src, slot) in n["links"].items():
            inputs[inp] = [str(src), slot]
        out[str(n["id"])] = {"class_type": n["cls"], "inputs": inputs}
    return out


if __name__ == "__main__":
    where = sys.argv[1]
    made = [
        (starter(False), "ProCraft - Z-Image new picture"),
        (starter(True), "ProCraft - Z-Image paint over"),
        (minimax(False), "ProCraft - MiniMax new picture"),
        (minimax(True), "ProCraft - MiniMax edit"),
        (rd_flow(), "ProCraft - Pixel art (RD Flow 64)"),
    ]
    for nodes, name in made:
        json.dump(to_ui(nodes, name), open(f"{where}/{name}.json", "w"), indent=2)
        json.dump(to_api(nodes), open(f"{where}/{name} (API).json", "w"), indent=2)
        print("made", name)
