<script lang="ts">
  import light from "../assets/logo-thanh-duy.svg";
  import dark from "../assets/logo-thanh-duy-dark.svg";

  let { size }: { size: number } = $props();
</script>

<!-- Logo vòng Enso, tự đổi bản sáng/tối theo giao diện của hệ điều hành. -->
<picture>
  <source srcset={dark} media="(prefers-color-scheme: dark)" />
  <img src={light} alt="" width={size} height={size} />
</picture>

<style>
  picture,
  img {
    display: block;
  }
</style>
